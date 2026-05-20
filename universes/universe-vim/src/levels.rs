//! Os 10 níveis progressivos do universo Vim.
//!
//! Cada nível ensina um conjunto de comandos Vim com texto em PT-BR.

/// Definição de um nível.
pub struct Level {
    pub number: usize,
    pub title: &'static str,
    pub description: &'static str,
    pub initial_text: &'static str,
    pub cursor_start: (usize, usize),
    pub success_fn: fn(lines: &[String], cursor: (usize, usize)) -> bool,
    pub hint: &'static str,
    pub seed_reward: u64,
}

/// Retorna todos os 10 níveis.
pub fn all_levels() -> Vec<Level> {
    vec![
        Level {
            number: 1,
            title: "Movimento básico — hjkl",
            description: "Mova o cursor até a posição marcada e pressione :w",
            initial_text: "Mova o cursor\naté a posição\n.... AQUI\ne pressione :w",
            cursor_start: (0, 0),
            success_fn: |_lines, cursor| cursor == (2, 4),
            hint: "Use j para descer linhas e l para avançar colunas. Chegue à linha 3, coluna 5.",
            seed_reward: 10,
        },
        Level {
            number: 2,
            title: "Apagar caractere — x",
            description: "Apague o 'x' extra na primeira linha",
            initial_text: "Corrija o errox\nnesta linha\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| !lines[0].contains('x'),
            hint: "Navegue até o 'x' extra e pressione x para apagá-lo.",
            seed_reward: 10,
        },
        Level {
            number: 3,
            title: "Apagar linha — dd",
            description: "Apague a linha 'REMOVA ESTA LINHA'",
            initial_text: "linha 1\nREMOVA ESTA LINHA\nlinha 3\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| !lines.join("\n").contains("REMOVA ESTA LINHA"),
            hint: "Mova o cursor para a linha com 'REMOVA ESTA LINHA' e pressione dd.",
            seed_reward: 10,
        },
        Level {
            number: 4,
            title: "Movimento por palavras — w/b",
            description: "Mova o cursor até o início da palavra 'destino'",
            initial_text: "va para o destino\nsem hjkl\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, cursor| {
                if cursor.0 == 0 && cursor.1 < lines[0].len() {
                    lines[0][cursor.1..].starts_with("destino")
                } else {
                    false
                }
            },
            hint: "Pressione w repetidamente para avançar por palavras até chegar em 'destino'.",
            seed_reward: 20,
        },
        Level {
            number: 5,
            title: "Modo inserção — i",
            description: "Insira 'sol' na primeira linha para que fique 'O sol brilha'",
            initial_text: "O  brilha\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| lines[0].contains("sol"),
            hint: "Mova o cursor para o espaço entre 'O' e 'brilha', pressione i e digite 'sol'.",
            seed_reward: 20,
        },
        Level {
            number: 6,
            title: "Acrescentar ao final — A",
            description: "Acrescente ' mundo' ao final de 'Olá'",
            initial_text: "Olá\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| lines[0] == "Olá mundo",
            hint: "Pressione A para entrar em modo Insert no final da linha e digite ' mundo'.",
            seed_reward: 20,
        },
        Level {
            number: 7,
            title: "Apagar palavra — dw",
            description: "Apague a palavra 'LIXO' da primeira linha",
            initial_text: "apagar LIXO aqui\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| !lines[0].contains("LIXO"),
            hint: "Mova até 'LIXO' com w, depois pressione dw para apagar a palavra.",
            seed_reward: 20,
        },
        Level {
            number: 8,
            title: "Copiar e colar — yy / p",
            description: "Duplique a linha 'copie esta linha'",
            initial_text: "copie esta linha\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| {
                lines.len() >= 2 && lines[0] == "copie esta linha" && lines[1] == "copie esta linha"
            },
            hint: "Pressione yy para copiar a linha, depois p para colar abaixo.",
            seed_reward: 30,
        },
        Level {
            number: 9,
            title: "Busca — /texto",
            description: "Use / para encontrar e posicionar o cursor em 'alvo'",
            initial_text: "linha um\nlinha dois\nalvo encontrado\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, cursor| {
                if cursor.0 < lines.len() && cursor.1 < lines[cursor.0].len() {
                    lines[cursor.0][cursor.1..].starts_with("alvo")
                } else {
                    false
                }
            },
            hint: "Digite /alvo e pressione Enter para saltar ao 'alvo'.",
            seed_reward: 30,
        },
        Level {
            number: 10,
            title: "Substituição — :s/errado/certo/",
            description: "Substitua 'errado' por 'certo' na primeira linha",
            initial_text: "texto errado aqui\npressione :w",
            cursor_start: (0, 0),
            success_fn: |lines, _cursor| lines[0].contains("certo") && !lines[0].contains("errado"),
            hint: "Digite :s/errado/certo/ e pressione Enter.",
            seed_reward: 30,
        },
    ]
}

/// Retorna a recompensa para o nível informado (sem bônus por ausência de hints).
pub fn base_reward(level_number: usize) -> u64 {
    match level_number {
        1..=3 => 10,
        4..=7 => 20,
        8..=10 => 30,
        _ => 0,
    }
}

// ── Tests de níveis ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ── Nível 1: hjkl ────────────────────────────────────────────────────────

    #[test]
    fn level1_success_at_correct_position() {
        let levels = all_levels();
        let l = &levels[0];
        let lines = strs(&[
            "Mova o cursor",
            "até a posição",
            ".... AQUI",
            "e pressione :w",
        ]);
        assert!((l.success_fn)(&lines, (2, 4)));
    }

    #[test]
    fn level1_fail_wrong_position() {
        let levels = all_levels();
        let l = &levels[0];
        let lines = strs(&[
            "Mova o cursor",
            "até a posição",
            ".... AQUI",
            "e pressione :w",
        ]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level1_fail_correct_row_wrong_col() {
        let levels = all_levels();
        let l = &levels[0];
        let lines = strs(&[
            "Mova o cursor",
            "até a posição",
            ".... AQUI",
            "e pressione :w",
        ]);
        assert!(!(l.success_fn)(&lines, (2, 3)));
    }

    // ── Nível 2: x ───────────────────────────────────────────────────────────

    #[test]
    fn level2_success_no_x_in_line0() {
        let levels = all_levels();
        let l = &levels[1];
        let lines = strs(&["Corrija o erro", "nesta linha", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level2_fail_x_still_present() {
        let levels = all_levels();
        let l = &levels[1];
        let lines = strs(&["Corrija o errox", "nesta linha", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level2_alternative_any_removal_works() {
        let levels = all_levels();
        let l = &levels[1];
        // Linha sem nenhum x também é válida
        let lines = strs(&["Corrigido", "nesta linha", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 3: dd ──────────────────────────────────────────────────────────

    #[test]
    fn level3_success_line_removed() {
        let levels = all_levels();
        let l = &levels[2];
        let lines = strs(&["linha 1", "linha 3", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level3_fail_line_still_present() {
        let levels = all_levels();
        let l = &levels[2];
        let lines = strs(&["linha 1", "REMOVA ESTA LINHA", "linha 3", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level3_alternative_any_removal() {
        let levels = all_levels();
        let l = &levels[2];
        let lines = strs(&["linha 1", "linha 3"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 4: w/b ─────────────────────────────────────────────────────────

    #[test]
    fn level4_success_cursor_at_destino() {
        let levels = all_levels();
        let l = &levels[3];
        let lines = strs(&["va para o destino", "sem hjkl", "pressione :w"]);
        // "destino" começa na col 10
        assert!((l.success_fn)(&lines, (0, 10)));
    }

    #[test]
    fn level4_fail_wrong_col() {
        let levels = all_levels();
        let l = &levels[3];
        let lines = strs(&["va para o destino", "sem hjkl", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level4_fail_wrong_row() {
        let levels = all_levels();
        let l = &levels[3];
        let lines = strs(&["va para o destino", "sem hjkl", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (1, 0)));
    }

    // ── Nível 5: i ───────────────────────────────────────────────────────────

    #[test]
    fn level5_success_sol_present() {
        let levels = all_levels();
        let l = &levels[4];
        let lines = strs(&["O sol brilha", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level5_fail_no_sol() {
        let levels = all_levels();
        let l = &levels[4];
        let lines = strs(&["O  brilha", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level5_alternative_sol_anywhere() {
        let levels = all_levels();
        let l = &levels[4];
        let lines = strs(&["sol brilha", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 6: A ───────────────────────────────────────────────────────────

    #[test]
    fn level6_success_exact_match() {
        let levels = all_levels();
        let l = &levels[5];
        let lines = strs(&["Olá mundo", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level6_fail_missing_mundo() {
        let levels = all_levels();
        let l = &levels[5];
        let lines = strs(&["Olá", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level6_fail_partial_match() {
        let levels = all_levels();
        let l = &levels[5];
        let lines = strs(&["Olá mun", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 7: dw ──────────────────────────────────────────────────────────

    #[test]
    fn level7_success_lixo_removed() {
        let levels = all_levels();
        let l = &levels[6];
        let lines = strs(&["apagar aqui", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level7_fail_lixo_present() {
        let levels = all_levels();
        let l = &levels[6];
        let lines = strs(&["apagar LIXO aqui", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level7_alternative_any_removal() {
        let levels = all_levels();
        let l = &levels[6];
        let lines = strs(&["apagar aqui"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 8: yy/p ────────────────────────────────────────────────────────

    #[test]
    fn level8_success_duplicated() {
        let levels = all_levels();
        let l = &levels[7];
        let lines = strs(&["copie esta linha", "copie esta linha", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level8_fail_not_duplicated() {
        let levels = all_levels();
        let l = &levels[7];
        let lines = strs(&["copie esta linha", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level8_fail_wrong_content() {
        let levels = all_levels();
        let l = &levels[7];
        let lines = strs(&["copie esta linha", "outra linha", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 9: /search ─────────────────────────────────────────────────────

    #[test]
    fn level9_success_cursor_at_alvo() {
        let levels = all_levels();
        let l = &levels[8];
        let lines = strs(&["linha um", "linha dois", "alvo encontrado", "pressione :w"]);
        assert!((l.success_fn)(&lines, (2, 0)));
    }

    #[test]
    fn level9_success_cursor_mid_alvo_line() {
        let levels = all_levels();
        let l = &levels[8];
        // Se "alvo" está mais à frente na linha
        let lines = strs(&[
            "linha um",
            "linha dois",
            "xx alvo encontrado",
            "pressione :w",
        ]);
        assert!((l.success_fn)(&lines, (2, 3)));
    }

    #[test]
    fn level9_fail_cursor_not_at_alvo() {
        let levels = all_levels();
        let l = &levels[8];
        let lines = strs(&["linha um", "linha dois", "alvo encontrado", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    // ── Nível 10: :s ─────────────────────────────────────────────────────────

    #[test]
    fn level10_success_certo_no_errado() {
        let levels = all_levels();
        let l = &levels[9];
        let lines = strs(&["texto certo aqui", "pressione :w"]);
        assert!((l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level10_fail_errado_still_present() {
        let levels = all_levels();
        let l = &levels[9];
        let lines = strs(&["texto errado aqui", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    #[test]
    fn level10_fail_no_certo() {
        let levels = all_levels();
        let l = &levels[9];
        let lines = strs(&["texto aqui", "pressione :w"]);
        assert!(!(l.success_fn)(&lines, (0, 0)));
    }

    // ── Recompensas ──────────────────────────────────────────────────────────

    #[test]
    fn test_base_reward_tiers() {
        assert_eq!(base_reward(1), 10);
        assert_eq!(base_reward(2), 10);
        assert_eq!(base_reward(3), 10);
        assert_eq!(base_reward(4), 20);
        assert_eq!(base_reward(5), 20);
        assert_eq!(base_reward(6), 20);
        assert_eq!(base_reward(7), 20);
        assert_eq!(base_reward(8), 30);
        assert_eq!(base_reward(9), 30);
        assert_eq!(base_reward(10), 30);
    }

    #[test]
    fn test_all_levels_count() {
        assert_eq!(all_levels().len(), 10);
    }

    #[test]
    fn test_all_levels_have_correct_numbers() {
        for (i, l) in all_levels().iter().enumerate() {
            assert_eq!(l.number, i + 1);
        }
    }
}
