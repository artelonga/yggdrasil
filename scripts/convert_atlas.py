#!/usr/bin/env python3
"""Converte atlas de núcleos (NIfTI labelmap) → Neuroglancer Precomputed.

Gera duas camadas em yggdrasil-web/static/neuro-data/ (servidas em /neuro):
  - ho-sub        : Harvard-Oxford subcortical (via nilearn)
  - aan-brainstem : Harvard AAN v2.0, núcleos do tronco (Zenodo, CC0)

Ambos em MNI152 1mm (mesma grade) → alinham no Neuroglancer.
Requer: cloud-volume, igneous-pipeline, nilearn, nibabel.
"""
import warnings; warnings.filterwarnings("ignore")
import os, shutil, json, gzip, glob, urllib.request
import numpy as np, nibabel as nib
from cloudvolume import CloudVolume
import igneous.task_creation as tc
from taskqueue import LocalTaskQueue

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DATA = os.path.join(ROOT, "yggdrasil-web", "static", "neuro-data")


def build_layer(name, data, zooms, names_by_id):
    """data: numpy labelmap; zooms: mm/voxel; names_by_id: {int: str}."""
    data = np.ascontiguousarray(data.astype(np.uint32))
    res_nm = [max(1, int(z * 1e6)) for z in zooms]
    ids = [int(x) for x in np.unique(data) if x != 0]
    dst = os.path.join(DATA, name)
    shutil.rmtree(dst, ignore_errors=True); os.makedirs(dst, exist_ok=True)
    print(f"[{name}] shape={data.shape} res_nm={res_nm} nuclei={len(ids)}")

    out = "file://" + dst
    info = CloudVolume.create_new_info(
        num_channels=1, layer_type="segmentation", data_type="uint32",
        encoding="raw", resolution=res_nm, voxel_offset=[0, 0, 0],
        chunk_size=[64, 64, 64], volume_size=list(data.shape), mesh="mesh")
    vol = CloudVolume(out, info=info); vol.commit_info()
    vol[:, :, :] = data[..., np.newaxis]

    # nomes dos segmentos (painel do NG)
    sp = os.path.join(dst, "segment_properties"); os.makedirs(sp, exist_ok=True)
    json.dump({"@type": "neuroglancer_segment_properties", "inline": {
        "ids": [str(i) for i in ids],
        "properties": [{"id": "label", "type": "label",
                        "values": [names_by_id.get(i, str(i)) for i in ids]}]}},
              open(os.path.join(sp, "info"), "w"))
    info2 = vol.info; info2["segment_properties"] = "segment_properties"
    vol.info = info2; vol.commit_info()

    tq = LocalTaskQueue(parallel=1)
    tq.insert(tc.create_meshing_tasks(out, mip=0, shape=(256, 256, 256), mesh_dir="mesh")); tq.execute()
    tq.insert(tc.create_mesh_manifest_tasks(out, mesh_dir="mesh")); tq.execute()

    # NG estático: igneous grava fragmentos .gz (espera storage com gzip
    # transparente). Descompacta para os nomes do manifesto.
    for gz in glob.glob(os.path.join(dst, "mesh", "*.gz")):
        with gzip.open(gz, "rb") as fi, open(gz[:-3], "wb") as fo:
            fo.write(fi.read())
        os.remove(gz)
    print(f"[{name}] ok → {dst} (ids {ids})")


def harvard_oxford():
    from nilearn import datasets
    ho = datasets.fetch_atlas_harvard_oxford("sub-maxprob-thr25-1mm")
    img = nib.load(ho["maps"]) if isinstance(ho["maps"], str) else ho["maps"]
    labels = ho["labels"]
    names = {i: labels[i] for i in range(len(labels))}
    build_layer("ho-sub", np.asarray(img.dataobj), img.header.get_zooms()[:3], names)


def aan_brainstem():
    base = "https://zenodo.org/api/records/8161638/files"
    nii = os.path.join(DATA, "_aan.nii"); lut = os.path.join(DATA, "_aan_lut.txt")
    os.makedirs(DATA, exist_ok=True)
    if not os.path.exists(nii):
        urllib.request.urlretrieve(f"{base}/AAN_Brainstem_MNI152_1mm_v2p0.nii/content", nii)
    if not os.path.exists(lut):
        urllib.request.urlretrieve(f"{base}/AAN_Brainstem_v2p0_Color_LUT_FreeSurfer.txt/content", lut)
    names = {}
    for line in open(lut):
        p = line.split()
        if len(p) >= 2 and p[0].isdigit():
            names[int(p[0])] = p[1]
    img = nib.load(nii)
    build_layer("aan-brainstem", np.asarray(img.dataobj), img.header.get_zooms()[:3], names)
    os.remove(nii); os.remove(lut)


if __name__ == "__main__":
    harvard_oxford()
    aan_brainstem()
    print("DONE — camadas:", os.listdir(DATA))
