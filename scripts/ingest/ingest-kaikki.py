#!/usr/bin/env python3
"""ingest-kaikki.py — consome o léxico Yoruba do kaikki (Wiktextract) → lexicon.yo.json.

Fase C do text-ingestion-roadmap. Emite o esquema canônico
(docs/architecture/ingestion-contract.md): {word, lang, gloss, pron, pop, examples}.
Passa pelo porteiro de soberania (source-gate.py yo kaikki-yoruba) antes de escrever.
Funde com as entradas curadas existentes (curado vence na glosa). Mantém o tom.

Uso:
  ingest-kaikki.py <kaikki.jsonl>            # escreve em <COMUNICACAO_DIR>/yoruba/lexicon.yo.json
  ingest-kaikki.py <kaikki.jsonl> --dry      # só conta, não escreve
Env: COMUNICACAO_DIR (default ../comunicacao).
"""
import json, os, subprocess, sys

MAX_SENSES = 3        # glosas por termo (Wiktionary é EN — fonte real)
MAX_EXAMPLES = 5
HERE = os.path.dirname(os.path.abspath(__file__))

def gate(lang, source_id):
    """Recusa-se a ingerir se o porteiro de soberania bloquear."""
    r = subprocess.run([sys.executable, os.path.join(HERE, "source-gate.py"), lang, source_id])
    if r.returncode != 0:
        sys.exit(f"BLOQUEADO pelo porteiro de soberania ({lang}/{source_id}) — abortando.")

def parse_kaikki(path):
    """JSONL Wiktextract → {word: entry}. Uma linha por word+pos; funde por word."""
    out = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            word = (rec.get("word") or "").strip()
            if not word or rec.get("lang_code") not in (None, "yo"):
                continue
            e = out.setdefault(word, {"word": word, "lang": "yo", "glosses": [], "examples": [], "pron": None})
            # pronúncia (IPA) — primeira encontrada
            if e["pron"] is None:
                for s in rec.get("sounds", []) or []:
                    if s.get("ipa"):
                        e["pron"] = s["ipa"]; break
            for sense in rec.get("senses", []) or []:
                for g in (sense.get("glosses") or sense.get("raw_glosses") or []):
                    if g and g not in e["glosses"]:
                        e["glosses"].append(g)
                for ex in sense.get("examples", []) or []:
                    t = (ex.get("text") or "").strip()
                    if t:
                        e["examples"].append({"gn": t, "pt": (ex.get("english") or "").strip()})
    return out

def to_entry(e):
    gloss = "; ".join(e["glosses"][:MAX_SENSES]) or None
    examples = e["examples"][:MAX_EXAMPLES]
    # pop = proxy de riqueza (nº de sentidos + exemplos) — vira rank no layout
    pop = len(e["glosses"]) + len(e["examples"])
    out = {"word": e["word"], "lang": "yo", "pop": pop}
    if gloss: out["gloss"] = gloss
    if e["pron"]: out["pron"] = e["pron"]
    if examples: out["examples"] = examples
    return out

def main():
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(2)
    src = sys.argv[1]
    dry = "--dry" in sys.argv[2:]
    gate("yo", "kaikki-yoruba")

    root = os.environ.get("COMUNICACAO_DIR", "../comunicacao")
    target = os.path.join(root, "yoruba", "lexicon.yo.json")

    parsed = parse_kaikki(src)
    entries = {w: to_entry(e) for w, e in parsed.items()}
    print(f"kaikki: {len(entries)} termos Yoruba")

    # funde com as entradas curadas (curado vence: glosa rica feita à mão)
    curated = []
    if os.path.exists(target):
        with open(target, encoding="utf-8") as f:
            curated = json.load(f)
    kept = 0
    for c in curated:
        w = c.get("word")
        if not w:
            continue
        base = entries.get(w, {"word": w, "lang": "yo", "pop": 0})
        # curado sobrescreve glosa/pron; pop = o maior
        if c.get("gloss"): base["gloss"] = c["gloss"]
        if c.get("pron"): base["pron"] = c["pron"]
        base["pop"] = max(int(base.get("pop", 0)), int(c.get("pop", 0)), 5)  # curado central
        entries[w] = base
        kept += 1
    print(f"curado fundido: {kept} entradas preservadas")

    rows = sorted(entries.values(), key=lambda r: r.get("pop", 0), reverse=True)
    print(f"total: {len(rows)} termos (ordenado por pop)")
    if dry:
        print("--dry: nada escrito"); return
    tmp = target + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(rows, f, ensure_ascii=False, indent=0)
    os.replace(tmp, target)
    print(f"escrito: {target}")

if __name__ == "__main__":
    main()
