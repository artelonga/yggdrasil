#!/usr/bin/env python3
"""source-gate.py — porteiro de soberania da ingestão (Fase B).

Lê o manifesto de fontes de uma língua (`<COMUNICACAO_DIR>/<lang>/_sources.yaml`)
e decide se uma fonte pode ser consumida. REGRA não-violável: fonte **sagrada**
(`sacred: true`) só passa com `custodian_consent: yes` (CARE Principles — as
custódias definem o acesso, não nós). Texto fabricado nunca; sem manifesto, nega.

Uso:
  source-gate.py <lang>                 # lista as fontes e o status (allow/BLOCK)
  source-gate.py <lang> <source-id>     # exit 0 se liberado; !=0 + motivo se não
Env: COMUNICACAO_DIR (default ../comunicacao).
"""
import os, sys

def _load_yaml(path):
    try:
        import yaml  # type: ignore
        with open(path, encoding="utf-8") as f:
            return yaml.safe_load(f)
    except ModuleNotFoundError:
        return _mini_yaml(path)  # fallback sem PyYAML

def _mini_yaml(path):
    """Parser mínimo p/ o formato restrito de _sources.yaml (sem dep externa)."""
    lang, sources, cur = None, [], None
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            if line.startswith("lang:"):
                lang = line.split(":", 1)[1].strip()
            elif line.lstrip().startswith("- "):
                cur = {}
                sources.append(cur)
                kv = line.lstrip()[2:]
                if ":" in kv:
                    k, v = kv.split(":", 1)
                    cur[k.strip()] = v.strip().strip('"')
            elif cur is not None and ":" in line and line.startswith(" "):
                k, v = line.split(":", 1)
                cur[k.strip()] = v.strip().strip('"')
    return {"lang": lang, "sources": sources}

def gate(source):
    """(allowed: bool, reason: str)."""
    # YAML 1.1 dobra yes/no→bool; aceitar ambas as formas (str e bool).
    sacred = str(source.get("sacred", False)).strip().lower() in ("true", "1", "yes")
    consent_raw = str(source.get("custodian_consent", "na")).strip().lower()
    consent_given = consent_raw in ("yes", "true", "1")
    if sacred and not consent_given:
        return False, f"SAGRADO sem consentimento das custódias (custodian_consent={consent_raw}) — CARE"
    return True, "ok"

def main():
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(2)
    lang = sys.argv[1]
    # código de língua → diretório do universo (espelha public.rs::lang_file)
    lang_dir = {"yo": "yoruba", "gn-mbya": "guarani-mbya", "gn": "guarani-mbya"}.get(lang, lang)
    root = os.environ.get("COMUNICACAO_DIR", "../comunicacao")
    path = os.path.join(root, lang_dir, "_sources.yaml")
    if not os.path.exists(path):
        print(f"sem manifesto: {path} — ingestão NEGADA (declare a fonte primeiro)")
        sys.exit(3)
    doc = _load_yaml(path)
    sources = doc.get("sources", []) if doc else []
    if len(sys.argv) >= 3:
        sid = sys.argv[2]
        src = next((s for s in sources if s.get("id") == sid), None)
        if not src:
            print(f"fonte '{sid}' não declarada em {path}"); sys.exit(3)
        ok, reason = gate(src)
        print(("ALLOW " if ok else "BLOCK ") + sid + " — " + reason)
        sys.exit(0 if ok else 1)
    # listagem
    print(f"# fontes de {lang} ({path})")
    for s in sources:
        ok, reason = gate(s)
        print(f"  [{'ALLOW' if ok else 'BLOCK'}] {s.get('id'):<16} {s.get('kind',''):<8} {reason}")

if __name__ == "__main__":
    main()
