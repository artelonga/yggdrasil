// Dino — jogo 3D (YG-180). Client-side, Three.js. Spawne como um dino e lute
// contra NPCs da MESMA espécie/atributos (luta justa). Modelos: placeholder
// procedural low-poly + glTF loader cabeado (assets open-source CC0 drop-in em
// /static/universos/assets/dino/dino.glb).
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

// ── parâmetros (iguais p/ jogador e NPC = luta justa) ───────────────────────
const ATTR = {
  maxHp: 100, speed: 7.5, jump: 8.5, gravity: 24,
  atkRange: 3.4, atkCos: 0.45, atkDmg: 22, atkCd: 0.62,
  blockDur: 0.5, blockMult: 0.12, dodgeImpulse: 9,
};
const NPC_COUNT = 4;
const AGGRO = 22, WORLD = 70;
const SENS = 0.0024;

const $ = (s) => document.querySelector(s);

// ── three básico ────────────────────────────────────────────────────────────
const canvas = $('#c');
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.shadowMap.enabled = true;
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x8fb6d6);
scene.fog = new THREE.Fog(0x8fb6d6, 60, 140);
const camera = new THREE.PerspectiveCamera(62, innerWidth / innerHeight, 0.1, 400);

function resize() {
  renderer.setSize(innerWidth, innerHeight);
  camera.aspect = innerWidth / innerHeight;
  camera.updateProjectionMatrix();
}
addEventListener('resize', resize); resize();

// luz
const hemi = new THREE.HemisphereLight(0xcfe6ff, 0x4a6b3a, 0.9);
scene.add(hemi);
const sun = new THREE.DirectionalLight(0xfff2d6, 1.1);
sun.position.set(30, 50, 20); sun.castShadow = true;
sun.shadow.mapSize.set(1024, 1024);
sun.shadow.camera.left = -80; sun.shadow.camera.right = 80;
sun.shadow.camera.top = 80; sun.shadow.camera.bottom = -80;
scene.add(sun);

// mundo simples: chão + cenário low-poly decorativo
const ground = new THREE.Mesh(
  new THREE.PlaneGeometry(WORLD * 2, WORLD * 2),
  new THREE.MeshStandardMaterial({ color: 0x6f9a52 }),
);
ground.rotation.x = -Math.PI / 2; ground.receiveShadow = true;
scene.add(ground);

function scatter() {
  const rng = (a, b) => a + Math.random() * (b - a);
  for (let i = 0; i < 36; i++) {
    const r = rng(8, WORLD - 6), a = rng(0, Math.PI * 2);
    const x = Math.cos(a) * r, z = Math.sin(a) * r;
    if (Math.random() < 0.5) {
      // pedra
      const m = new THREE.Mesh(
        new THREE.IcosahedronGeometry(rng(0.6, 1.8), 0),
        new THREE.MeshStandardMaterial({ color: 0x8a8a86, flatShading: true }),
      );
      m.position.set(x, rng(0.3, 0.7), z); m.castShadow = true; scene.add(m);
    } else {
      // "árvore" low-poly (tronco + copa cônica)
      const t = new THREE.Group();
      const trunk = new THREE.Mesh(new THREE.CylinderGeometry(0.22, 0.3, 1.6, 6),
        new THREE.MeshStandardMaterial({ color: 0x6b4a2f }));
      trunk.position.y = 0.8;
      const top = new THREE.Mesh(new THREE.ConeGeometry(rng(1.1, 1.8), rng(2.2, 3.5), 7),
        new THREE.MeshStandardMaterial({ color: 0x3f7d3a, flatShading: true }));
      top.position.y = 2.6;
      t.add(trunk, top); t.position.set(x, 0, z);
      t.traverse((o) => (o.castShadow = true));
      scene.add(t);
    }
  }
}
scatter();

// ── modelo do dino: glTF real (se existir) OU placeholder procedural ─────────
let modelProto = null;
async function tryLoadModel() {
  try {
    const gltf = await new GLTFLoader().loadAsync('/static/universos/assets/dino/dino.glb');
    gltf.scene.traverse((o) => { if (o.isMesh) o.castShadow = true; });
    modelProto = gltf.scene;
  } catch { modelProto = null; } // sem asset → placeholder
}

function box(w, h, d, color) {
  return new THREE.Mesh(new THREE.BoxGeometry(w, h, d),
    new THREE.MeshStandardMaterial({ color, flatShading: true }));
}
// dino procedural: pés em y=0, frente = -Z. Cor levemente variada por indivíduo,
// mas MESMA forma/tamanho (mesma espécie).
function makeDino(tint) {
  if (modelProto) {
    const m = modelProto.clone(true);
    m.userData.proc = false;
    return m;
  }
  const g = new THREE.Group();
  const body = box(1.0, 0.9, 1.9, tint); body.position.set(0, 1.05, 0);
  const neck = box(0.5, 0.9, 0.5, tint); neck.position.set(0, 1.7, -0.85); neck.rotation.x = 0.35;
  const head = box(0.6, 0.55, 0.9, tint); head.position.set(0, 2.05, -1.35);
  const snout = box(0.42, 0.32, 0.5, tint); snout.position.set(0, 1.95, -1.85);
  const eyeL = box(0.1, 0.1, 0.1, 0x111111); eyeL.position.set(0.2, 2.15, -1.55);
  const eyeR = eyeL.clone(); eyeR.position.x = -0.2;
  const tail = box(0.45, 0.45, 1.6, tint); tail.position.set(0, 1.0, 1.5); tail.rotation.x = -0.15;
  const tail2 = box(0.25, 0.25, 1.2, tint); tail2.position.set(0, 0.85, 2.6);
  const mk = (x, z) => { const l = box(0.32, 1.0, 0.4, tint); l.position.set(x, 0.5, z); return l; };
  const legs = [mk(0.42, -0.4), mk(-0.42, -0.4), mk(0.42, 0.5), mk(-0.42, 0.5)];
  g.add(body, neck, head, snout, eyeL, eyeR, tail, tail2, ...legs);
  g.traverse((o) => { if (o.isMesh) o.castShadow = true; });
  g.userData.proc = true;
  g.userData.legs = legs; g.userData.tail = tail;
  return g;
}

// ── entidades ────────────────────────────────────────────────────────────────
function makeEntity(isPlayer, x, z, tint) {
  const group = makeDino(tint);
  group.position.set(x, 0, z);
  scene.add(group);
  return {
    group, isPlayer, alive: true,
    hp: ATTR.maxHp, maxHp: ATTR.maxHp,
    vy: 0, onGround: true, yaw: isPlayer ? 0 : Math.random() * Math.PI * 2,
    atkCd: 0, atkAnim: 0, blockT: 0, moving: 0,
    ai: isPlayer ? null : { state: 'wander', t: 0, tx: x, tz: z },
    plate: isPlayer ? null : makePlate(),
  };
}

let player, npcs = [];
function spawnAll() {
  player = makeEntity(true, 0, 8, 0xe9c349);
  npcs = [];
  for (let i = 0; i < NPC_COUNT; i++) {
    const a = (i / NPC_COUNT) * Math.PI * 2, r = 18 + Math.random() * 12;
    const tints = [0x9c6b3f, 0x7a8b5a, 0x8a5a4a, 0x6a7b8a];
    npcs.push(makeEntity(false, Math.cos(a) * r, Math.sin(a) * r, tints[i % tints.length]));
  }
}

// vetor de frente a partir do yaw (frente = -Z)
function fwd(yaw) { return new THREE.Vector3(-Math.sin(yaw), 0, -Math.cos(yaw)); }
function rightOf(yaw) { return new THREE.Vector3(Math.cos(yaw), 0, -Math.sin(yaw)); }

// ── HUD ──────────────────────────────────────────────────────────────────────
function makePlate() {
  const el = document.createElement('div'); el.className = 'nameplate';
  el.innerHTML = '<div class="nm">dino</div><div class="track"><div class="fill"></div></div>';
  $('#nameplates').appendChild(el);
  return el;
}
function buildBars() {
  const hb = $('#hotbar'); hb.innerHTML = '';
  for (let i = 0; i < 9; i++) {
    const s = document.createElement('div');
    s.className = 'slot' + (i === 0 ? ' sel' : '');
    s.innerHTML = '<span class="n">' + (i + 1) + '</span>';
    hb.appendChild(s);
  }
  const ig = $('#inv-grid'); ig.innerHTML = '';
  for (let i = 0; i < 27; i++) { const s = document.createElement('div'); s.className = 'slot'; ig.appendChild(s); }
}
let selSlot = 0;
function selectSlot(i) {
  selSlot = (i + 9) % 9;
  document.querySelectorAll('#hotbar .slot').forEach((s, k) => s.classList.toggle('sel', k === selSlot));
}

// ── input ────────────────────────────────────────────────────────────────────
const keys = {};
let locked = false, running = false;
addEventListener('keydown', (e) => {
  keys[e.code] = true;
  if (e.code === 'KeyE' || e.code === 'KeyI') $('#inv').classList.toggle('open');
  if (e.code >= 'Digit1' && e.code <= 'Digit9') selectSlot(+e.code.slice(5) - 1);
});
addEventListener('keyup', (e) => { keys[e.code] = false; });
addEventListener('wheel', (e) => { if (running) selectSlot(selSlot + (e.deltaY > 0 ? 1 : -1)); }, { passive: true });

canvas.addEventListener('click', () => { if (running && !locked) canvas.requestPointerLock(); });
document.addEventListener('pointerlockchange', () => { locked = document.pointerLockElement === canvas; });
addEventListener('mousemove', (e) => {
  if (!locked || !player) return;
  player.yaw -= e.movementX * SENS;
  pitch = Math.max(-0.45, Math.min(0.8, pitch - e.movementY * SENS));
});
addEventListener('contextmenu', (e) => e.preventDefault());
addEventListener('mousedown', (e) => {
  if (!running || !locked || !player.alive) return;
  if (e.button === 0) attack(player);
  else if (e.button === 2) blockDodge(player);
});
let pitch = 0.15;

// ── combate ──────────────────────────────────────────────────────────────────
function attack(e) {
  if (e.atkCd > 0 || !e.alive) return;
  e.atkCd = ATTR.atkCd; e.atkAnim = 0.28;
  const f = fwd(e.yaw), origin = e.group.position;
  const targets = e.isPlayer ? npcs : [player];
  for (const t of targets) {
    if (!t.alive) continue;
    const to = t.group.position.clone().sub(origin); to.y = 0;
    const dist = to.length();
    if (dist > ATTR.atkRange) continue;
    if (f.dot(to.normalize()) < ATTR.atkCos) continue; // fora do arco
    damage(t, ATTR.atkDmg);
  }
}
function blockDodge(e) {
  if (e.blockT > 0 || !e.alive) return;
  e.blockT = ATTR.blockDur;
  // esquiva: impulso lateral/para trás (sai da linha do ataque)
  const dir = fwd(e.yaw).multiplyScalar(-1);
  e.dodgeVX = dir.x * ATTR.dodgeImpulse; e.dodgeVZ = dir.z * ATTR.dodgeImpulse;
}
function damage(t, amount) {
  if (!t.alive) return;
  if (t.blockT > 0) amount *= ATTR.blockMult; // bloqueio/esquiva = i-frames + redução
  t.hp -= amount;
  if (t.hp <= 0) { t.hp = 0; t.alive = false; onDeath(t); }
}
function onDeath(t) {
  t.group.rotation.z = Math.PI / 2; // tomba
  t.group.position.y = 0.4;
  if (t.plate) t.plate.style.display = 'none';
  if (t.isPlayer) endGame(false);
  else if (npcs.every((n) => !n.alive)) endGame(true);
}

// ── update ───────────────────────────────────────────────────────────────────
function moveEntity(e, dx, dz, dt) {
  // dash da esquiva decai
  if (e.dodgeVX) { dx += e.dodgeVX; dz += e.dodgeVZ; e.dodgeVX *= 0.82; e.dodgeVZ *= 0.82; if (Math.abs(e.dodgeVX) < 0.3) e.dodgeVX = e.dodgeVZ = 0; }
  const p = e.group.position;
  p.x = Math.max(-WORLD + 2, Math.min(WORLD - 2, p.x + dx * dt));
  p.z = Math.max(-WORLD + 2, Math.min(WORLD - 2, p.z + dz * dt));
  // gravidade / pulo
  e.vy -= ATTR.gravity * dt; p.y += e.vy * dt;
  if (p.y <= 0) { p.y = 0; e.vy = 0; e.onGround = true; }
  e.group.rotation.y = e.yaw;
  e.moving = Math.hypot(dx, dz) > 0.5 ? Math.min(1, e.moving + dt * 4) : Math.max(0, e.moving - dt * 4);
}
function animate(e, time) {
  if (!e.alive) return;
  const g = e.group;
  if (g.userData.proc) {
    const sw = Math.sin(time * 10) * 0.5 * e.moving;
    const legs = g.userData.legs;
    if (legs) { legs[0].rotation.x = sw; legs[3].rotation.x = sw; legs[1].rotation.x = -sw; legs[2].rotation.x = -sw; }
    if (g.userData.tail) g.userData.tail.rotation.y = Math.sin(time * 5) * 0.18;
  }
  // lunge do ataque
  g.rotation.x = e.atkAnim > 0 ? -Math.sin((1 - e.atkAnim / 0.28) * Math.PI) * 0.4 : 0;
}

function updatePlayer(dt) {
  const p = player; if (!p.alive) return;
  let dx = 0, dz = 0;
  const f = fwd(p.yaw), r = rightOf(p.yaw);
  if (keys.KeyW || keys.ArrowUp) { dx += f.x; dz += f.z; }
  if (keys.KeyS || keys.ArrowDown) { dx -= f.x; dz -= f.z; }
  if (keys.KeyD || keys.ArrowRight) { dx += r.x; dz += r.z; }
  if (keys.KeyA || keys.ArrowLeft) { dx -= r.x; dz -= r.z; }
  const len = Math.hypot(dx, dz); if (len > 0) { dx = dx / len * ATTR.speed; dz = dz / len * ATTR.speed; }
  if ((keys.Space) && p.onGround) { p.vy = ATTR.jump; p.onGround = false; }
  moveEntity(p, dx, dz, dt);
  if (p.atkCd > 0) p.atkCd -= dt;
  if (p.atkAnim > 0) p.atkAnim -= dt;
  if (p.blockT > 0) p.blockT -= dt;
}

function updateNpc(n, dt) {
  if (!n.alive) return;
  const toP = player.group.position.clone().sub(n.group.position); toP.y = 0;
  const dist = toP.length();
  let dx = 0, dz = 0;
  n.ai.t -= dt;
  if (player.alive && dist < AGGRO) {
    // perseguir / atacar
    n.yaw = Math.atan2(-toP.x, -toP.z); // encara o jogador (frente = -Z)
    if (dist > ATTR.atkRange * 0.85) { const d = toP.normalize(); dx = d.x * ATTR.speed; dz = d.z * ATTR.speed; }
    else if (n.atkCd <= 0) attack(n);
  } else {
    // vagar
    if (n.ai.t <= 0) { const a = Math.random() * Math.PI * 2, r = 6 + Math.random() * 14; n.ai.tx = n.group.position.x + Math.cos(a) * r; n.ai.tz = n.group.position.z + Math.sin(a) * r; n.ai.t = 2 + Math.random() * 3; }
    const to = new THREE.Vector3(n.ai.tx - n.group.position.x, 0, n.ai.tz - n.group.position.z);
    if (to.length() > 1) { n.yaw = Math.atan2(-to.x, -to.z); to.normalize(); dx = to.x * ATTR.speed * 0.4; dz = to.z * ATTR.speed * 0.4; }
  }
  moveEntity(n, dx, dz, dt);
  if (n.atkCd > 0) n.atkCd -= dt;
  if (n.atkAnim > 0) n.atkAnim -= dt;
  if (n.blockT > 0) n.blockT -= dt;
}

// câmera 3ª pessoa
function updateCamera() {
  const f = fwd(player.yaw);
  const head = player.group.position.clone().add(new THREE.Vector3(0, 2.2, 0));
  const dist = 8, height = 3.5 + pitch * 6;
  camera.position.copy(head).addScaledVector(f, -dist).add(new THREE.Vector3(0, height, 0));
  camera.lookAt(head);
}

// projetar nameplates + barras
const _v = new THREE.Vector3();
function updateHud() {
  $('#hp-fill').style.width = (player.hp / player.maxHp * 100) + '%';
  $('#player-state').textContent = player.blockT > 0 ? '⛊ bloqueando / esquivando' : (player.atkCd > 0.4 ? '⚔ atacando' : '');
  $('#alive').textContent = npcs.filter((n) => n.alive).length;
  for (const n of npcs) {
    if (!n.alive) continue;
    _v.copy(n.group.position).add(new THREE.Vector3(0, 3.2, 0)).project(camera);
    const onScreen = _v.z < 1 && _v.x > -1 && _v.x < 1 && _v.y > -1 && _v.y < 1;
    n.plate.style.display = onScreen ? 'block' : 'none';
    if (onScreen) {
      n.plate.style.left = (_v.x * 0.5 + 0.5) * innerWidth + 'px';
      n.plate.style.top = (-_v.y * 0.5 + 0.5) * innerHeight + 'px';
      n.plate.querySelector('.fill').style.width = (n.hp / n.maxHp * 100) + '%';
    }
  }
}

// ── loop ─────────────────────────────────────────────────────────────────────
const clock = new THREE.Clock();
function frame() {
  requestAnimationFrame(frame);
  const dt = Math.min(clock.getDelta(), 0.05);
  const time = clock.elapsedTime;
  if (running) {
    updatePlayer(dt);
    for (const n of npcs) updateNpc(n, dt);
    animate(player, time);
    for (const n of npcs) animate(n, time);
    updateCamera();
    updateHud();
  }
  renderer.render(scene, camera);
}

// ── controle de jogo ─────────────────────────────────────────────────────────
function endGame(won) {
  running = false; document.exitPointerLock?.();
  $('#end-title').textContent = won ? 'Você venceu' : 'Você caiu';
  $('#end-sub').textContent = won ? 'Todos os dinos do vale foram derrotados.' : 'Outro dino da sua espécie levou a melhor. Tente de novo.';
  $('#end').classList.remove('hidden');
}
function reset() {
  // remove entidades antigas
  if (player) scene.remove(player.group);
  for (const n of npcs) { scene.remove(n.group); n.plate?.remove(); }
  spawnAll();
  pitch = 0.15; selectSlot(0);
  $('#end').classList.add('hidden');
}
function startGame() {
  if (!player) reset();
  $('#start').classList.add('hidden');
  running = true; canvas.requestPointerLock();
}

$('#start-btn').addEventListener('click', startGame);
$('#end-btn').addEventListener('click', () => { reset(); running = true; canvas.requestPointerLock(); });

buildBars();
tryLoadModel().finally(() => { spawnAll(); frame(); });
