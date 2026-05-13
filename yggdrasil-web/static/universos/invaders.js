'use strict';

const W = 480, H = 480;
const ALIEN_W = 32, ALIEN_H = 24;
const PLAYER_W = 40, PLAYER_H = 10;
const PLAYER_Y = H - 30;
const BULLET_W = 3, BULLET_H = 10;

const state = {
    gameId: null,
    playerX: W / 2 - PLAYER_W / 2,
    bullets: [],
    aliens: [],
    score: 0,
    lives: 3,
    gameOver: false,
    won: false,
    canvas: null,
    ctx: null,
    animFrame: null,
    busy: false,
};

let playerVx = 0;
let pendingShoot = false;

function setStatus(msg) {
    const el = document.getElementById('rodape');
    if (el) el.textContent = msg;
}

function setScore(s) {
    const el = document.getElementById('score');
    if (el) el.textContent = s;
}

function setLives(l) {
    const el = document.getElementById('lives');
    if (el) el.textContent = l;
}

function draw() {
    const { ctx } = state;
    if (!ctx) return;

    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, W, H);

    if (!state.gameOver && !state.won) {
        // Player
        ctx.fillStyle = '#34d399';
        ctx.fillRect(state.playerX, PLAYER_Y, PLAYER_W, PLAYER_H);

        // Aliens
        state.aliens.forEach(a => {
            if (!a.alive) return;
            ctx.fillStyle = a.row < 2 ? '#e0505f' : '#60a5fa';
            ctx.fillRect(a.x + 6, a.y, ALIEN_W - 12, ALIEN_H - 8);
            ctx.fillRect(a.x, a.y + 6, ALIEN_W, ALIEN_H - 14);
            ctx.fillRect(a.x + 2, a.y + ALIEN_H - 8, 8, 8);
            ctx.fillRect(a.x + ALIEN_W - 10, a.y + ALIEN_H - 8, 8, 8);
        });

        // Bullets
        state.bullets.forEach(b => {
            ctx.fillStyle = b.alien ? '#E9C349' : '#fff';
            ctx.fillRect(b.x - BULLET_W / 2, b.y, BULLET_W, BULLET_H);
        });
    }

    if (state.gameOver || state.won) {
        ctx.fillStyle = 'rgba(0,0,0,0.75)';
        ctx.fillRect(0, 0, W, H);
        ctx.fillStyle = '#fff';
        ctx.font = 'bold 22px monospace';
        ctx.textAlign = 'center';
        ctx.fillText(state.won ? 'VITÓRIA!' : 'GAME OVER', W / 2, H / 2 - 20);
        ctx.font = '14px monospace';
        ctx.fillText(`Pontuação: ${state.score}`, W / 2, H / 2 + 10);
        ctx.fillText('Voltando ao lobby...', W / 2, H / 2 + 35);
    }
}

async function sendInput(direction) {
    if (state.gameOver || state.won || state.busy || !state.gameId) return;
    state.busy = true;

    try {
        const res = await fetch(`/api/v1/games/invaders/${state.gameId}/input`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ direction }),
        });
        if (!res.ok) return;
        const data = await res.json();

        const s = data.state;
        state.playerX = s.player_x;
        state.bullets  = s.bullets;
        state.aliens   = s.aliens;
        state.score    = data.score;
        state.lives    = s.lives;
        state.gameOver = s.game_over;
        state.won      = s.won;

        setScore(data.score);
        setLives(s.lives);
        draw();

        if (data.action === 'quit') {
            draw();
            setStatus('Redirecionando para o lobby...');
            setTimeout(() => window.location.assign('/lobby'), 2000);
        }
    } catch (e) {
        console.error('invaders input error', e);
    } finally {
        state.busy = false;
    }
}

function gameLoop() {
    if (!state.gameOver && !state.won) {
        let input;
        if (pendingShoot) {
            input = 'Shoot';
            pendingShoot = false;
        } else if (playerVx < 0) {
            input = 'Left';
        } else if (playerVx > 0) {
            input = 'Right';
        } else {
            input = 'Tick';
        }
        sendInput(input);
    }
    state.animFrame = requestAnimationFrame(gameLoop);
}

function handleKeyDown(e) {
    if (state.gameOver || state.won) return;
    switch (e.key) {
        case 'ArrowLeft':
        case 'a':
        case 'A':
            e.preventDefault();
            playerVx = -4;
            break;
        case 'ArrowRight':
        case 'd':
        case 'D':
            e.preventDefault();
            playerVx = 4;
            break;
        case ' ':
        case 'ArrowUp':
            e.preventDefault();
            pendingShoot = true;
            break;
        case 'Escape':
        case 'q':
        case 'Q':
            sendInput('Quit');
            break;
        default:
            return;
    }
}

function handleKeyUp(e) {
    if (e.key === 'ArrowLeft' || e.key === 'a' || e.key === 'A') playerVx = 0;
    if (e.key === 'ArrowRight' || e.key === 'd' || e.key === 'D') playerVx = 0;
}

async function initInvaders() {
    const res = await fetch('/api/v1/games/invaders/start');
    if (!res.ok) throw new Error(`start failed: ${res.status}`);
    const data = await res.json();

    state.gameId   = data.id;
    const s        = data.state;
    state.playerX  = s.player_x;
    state.bullets  = s.bullets;
    state.aliens   = s.aliens;
    state.score    = data.score;
    state.lives    = s.lives;
    state.gameOver = s.game_over;
    state.won      = s.won;

    const canvas   = document.getElementById('canvas');
    canvas.width   = W;
    canvas.height  = H;
    state.canvas   = canvas;
    state.ctx      = canvas.getContext('2d');

    draw();
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    state.animFrame = requestAnimationFrame(gameLoop);
    setStatus('');
}

initInvaders().catch(err => {
    setStatus('Erro ao iniciar jogo');
    console.error(err);
});
