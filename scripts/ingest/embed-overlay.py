#!/usr/bin/env python3
"""embed-overlay.py — overlay semântico NEURAL (local-first) via Ollama embeddings.

Fase 3 do topologia-roadmap, model-agnostic: pega os nós do servidor (/nos),
embeda o contexto (glosa) com um modelo LOCAL (Ollama; default nomic-embed-text),
calcula o cosseno top-k por nó e grava como `method='neural'` na MESMA tabela
`topo_semantic` (reusa toda a infra de overlay — sem mudança de storage). O modelo
e o corpus nunca saem da máquina.

Uso:
  embed-overlay.py [--base http://localhost:8175] [--db <YGGDRASIL_DB>]
                   [--model nomic-embed-text] [--top 600] [--k 8] [--threshold 0.55]
Pré: ollama serve + o modelo pulado; o servidor yggdrasil no ar (p/ /nos).
"""
import argparse, json, sqlite3, time, urllib.request

def http_json(url, payload=None):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())

def embed(base_ollama, model, text):
    d = http_json(base_ollama + "/api/embeddings", {"model": model, "prompt": text})
    return d.get("embedding") or []

def cosine(a, b):
    return sum(x * y for x, y in zip(a, b))  # vetores já normalizados

def normalize(v):
    n = sum(x * x for x in v) ** 0.5
    return [x / n for x in v] if n else v

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://localhost:8175")
    ap.add_argument("--ollama", default="http://localhost:11434")
    ap.add_argument("--db", required=True)
    ap.add_argument("--model", default="nomic-embed-text")
    ap.add_argument("--top", type=int, default=600)
    ap.add_argument("--k", type=int, default=8)
    ap.add_argument("--threshold", type=float, default=0.55)
    a = ap.parse_args()

    nodes = http_json(a.base + "/api/v1/topologia/nos")
    nodes = [n for n in nodes if n.get("gloss")]
    nodes.sort(key=lambda n: n.get("pop", 0), reverse=True)
    nodes = nodes[: a.top]
    print(f"embedando {len(nodes)} nós (top por pop, com glosa) via {a.model}…")

    vecs = []
    t0 = time.time()
    for i, n in enumerate(nodes):
        ctx = (n.get("term", "") + ": " + (n.get("gloss") or "")).strip()
        vecs.append((n["id"], normalize(embed(a.ollama, a.model, ctx))))
        if (i + 1) % 100 == 0:
            print(f"  {i+1}/{len(nodes)} ({time.time()-t0:.0f}s)")
    print(f"embeddings prontos em {time.time()-t0:.0f}s")

    # top-k cosseno por nó (par canônico, dedup)
    pairs, seen = [], set()
    for i in range(len(vecs)):
        ida, va = vecs[i]
        sims = []
        for j in range(len(vecs)):
            if i == j:
                continue
            s = cosine(va, vecs[j][1])
            if s >= a.threshold:
                sims.append((vecs[j][0], s))
        sims.sort(key=lambda x: -x[1])
        for idb, s in sims[: a.k]:
            key = tuple(sorted((ida, idb)))
            if key not in seen:
                seen.add(key)
                pairs.append((key[0], key[1], s))
    print(f"{len(pairs)} pares neural >= {a.threshold}")

    now = int(time.time() * 1000)
    conn = sqlite3.connect(a.db)
    conn.execute("DELETE FROM topo_semantic WHERE method='neural'")
    conn.executemany(
        "INSERT OR REPLACE INTO topo_semantic (a,b,score,method,computed_at) VALUES (?,?,?,'neural',?)",
        [(x[0], x[1], x[2], now) for x in pairs],
    )
    conn.commit(); conn.close()
    print(f"gravado em {a.db} (method='neural')")

if __name__ == "__main__":
    main()
