#!/usr/bin/env bash
# fetch-anatomy.sh — baixa e prepara as malhas 3D de anatomia (YG-83).
#
# Fonte: BodyParts3D / DBCLS (CC-BY-SA 2.1 Japan). Ver assets/anatomia/ATTRIBUTION.md.
# As malhas são grandes (~37 MB) e geradas, então não vão pro git — este script
# as reproduz a partir do arquivo público do DBCLS.
#
# Uso: yggdrasil-godot/scripts/fetch-anatomy.sh
#
# Produz em assets/anatomia/: body_skin.obj, brain.obj, spinal_cord.obj
# Requer: curl, unzip, node.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUT="${PROJECT_DIR}/assets/anatomia"
ZIP_URL="https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/partof_BP3D_4.0_obj_99.zip"
MAP_URL="https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/partof_element_parts.txt"
PREFIX="partof_BP3D_4.0_obj_99"

mkdir -p "${OUT}"
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

echo ">>> baixando BodyParts3D (partof, ~61 MB)..."
curl -fsSL "${ZIP_URL}" -o "${TMP}/bp3d.zip"
curl -fsSL "${MAP_URL}" -o "${TMP}/map.txt"

# concept id (FMA) → element file ids; tira CRLF do arquivo do DBCLS.
ids_for() { awk -F'\t' -v f="$1" '$1==f{print $3}' "${TMP}/map.txt" | tr -d '\r' | sort -u; }

extract() { # <fj-id> <dest.obj>
  unzip -o -j "${TMP}/bp3d.zip" "${PREFIX}/$1.obj" -d "${TMP}" >/dev/null 2>&1 || true
  [ -f "${TMP}/$1.obj" ] && cp "${TMP}/$1.obj" "$2"
}

echo ">>> extraindo pele (FMA7163) e medula (FMA7647)..."
extract "$(ids_for FMA7163 | head -1)" "${OUT}/body_skin.obj"
extract "$(ids_for FMA7647 | head -1)" "${OUT}/spinal_cord.obj"

echo ">>> extraindo e mesclando encéfalo (FMA50801, ~59 sub-malhas)..."
BRAIN_DIR="${TMP}/brain"; mkdir -p "${BRAIN_DIR}"
for fj in $(ids_for FMA50801); do
  unzip -o -j "${TMP}/bp3d.zip" "${PREFIX}/${fj}.obj" -d "${BRAIN_DIR}" >/dev/null 2>&1 || true
done
ls "${BRAIN_DIR}"/*.obj > "${TMP}/brainlist.txt"

# mescla os OBJs num só, re-indexando vértices/normais; normaliza faces p/ v//vn
# (o importador de OBJ do Godot rejeita formatos de face mistos).
node -e '
const fs=require("fs");
const files=fs.readFileSync(process.env.LIST,"utf8").trim().split("\n").filter(Boolean);
let out=[], vO=0, vnO=0;
for(const fp of files){
  let lv=0, lvn=0;
  for(const raw of fs.readFileSync(fp,"utf8").split("\n")){
    const t=raw.replace(/\r$/,"").trim(); if(!t||t[0]==="#")continue;
    const p=t.split(/\s+/).filter(Boolean);
    if(p[0]==="v"){lv++; out.push("v "+p[1]+" "+p[2]+" "+p[3]);}
    else if(p[0]==="vn"){lvn++; out.push("vn "+p[1]+" "+p[2]+" "+p[3]);}
    else if(p[0]==="f"){
      const f=["f"];
      for(let i=1;i<p.length;i++){const a=p[i].split("/"); f.push((parseInt(a[0])+vO)+"//"+(parseInt(a[2])+vnO));}
      out.push(f.join(" "));
    }
  }
  vO+=lv; vnO+=lvn;
}
fs.writeFileSync(process.env.OUT,"# BodyParts3D brain merged (CC-BY-SA 2.1 JP / DBCLS)\n"+out.join("\n")+"\n");
console.log("    brain.obj verts="+vO);
' LIST="${TMP}/brainlist.txt" OUT="${OUT}/brain.obj"

echo ">>> pronto:"
du -h "${OUT}"/body_skin.obj "${OUT}"/brain.obj "${OUT}"/spinal_cord.obj
echo ">>> abra o viewer:  ./scripts/godot.sh run res://scenes/anatomy3d/anatomy_viewer.tscn"
