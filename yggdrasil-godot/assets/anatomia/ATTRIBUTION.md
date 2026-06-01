# Atribuição — malhas 3D de anatomia

As malhas em `assets/anatomia/` derivam do **BodyParts3D**, do banco de dados
*Life Science Database* (DBCLS, Japão).

- **Fonte:** BodyParts3D, © The Database Center for Life Science (DBCLS).
- **Licença:** **CC-BY-SA 2.1 Japan** — atribuição + compartilhamento igual.
- **Download:** <https://dbarchive.biosciencedbc.jp/en/bodyparts3d/download.html>
  (`partof_BP3D_4.0_obj_99.zip`).

## Arquivos

| Arquivo | Estrutura | Origem (FMA / BodyParts3D) |
|---|---|---|
| `body_skin.obj` | Pele / superfície do corpo | FMA7163 (FJ2810) |
| `brain.obj` | Encéfalo (59 sub-malhas mescladas) | FMA50801 |
| `spinal_cord.obj` | Medula espinhal | FMA7647 (FJ1737) |

`brain.obj` foi gerado mesclando as 59 sub-malhas do encéfalo num único OBJ
(re-indexação de vértices) — ver `scripts/fetch-anatomy.sh`. As três compartilham
o mesmo referencial de coordenadas do BodyParts3D, então o SNC já fica
posicionado dentro do corpo sem alinhamento manual.

Como é CC-BY-**SA**, qualquer redistribuição destas malhas (ou derivadas) deve
manter a mesma licença e creditar o DBCLS/BodyParts3D.
