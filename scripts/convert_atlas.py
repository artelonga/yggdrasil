import warnings; warnings.filterwarnings("ignore")
import numpy as np, nibabel as nib, os, shutil
from nilearn import datasets
from cloudvolume import CloudVolume
import igneous.task_creation as tc
from taskqueue import LocalTaskQueue

ho = datasets.fetch_atlas_harvard_oxford('sub-maxprob-thr25-1mm')
img = nib.load(ho['maps']) if isinstance(ho['maps'], str) else ho['maps']
data = np.ascontiguousarray(np.asarray(img.dataobj).astype(np.uint32))
zooms = [float(z) for z in img.header.get_zooms()[:3]]
res_nm = [max(1,int(z*1e6)) for z in zooms]
ids = [int(x) for x in np.unique(data) if x != 0]
print("shape", data.shape, "res_nm", res_nm, "nuclei", len(ids))

dst = os.path.abspath('yggdrasil-web/static/neuro-data/ho-sub')
shutil.rmtree(dst, ignore_errors=True); os.makedirs(dst, exist_ok=True)
out = 'file://' + dst
info = CloudVolume.create_new_info(num_channels=1, layer_type='segmentation',
    data_type='uint32', encoding='raw', resolution=res_nm, voxel_offset=[0,0,0],
    chunk_size=[64,64,64], volume_size=list(data.shape), mesh='mesh')
vol = CloudVolume(out, info=info); vol.commit_info()
vol[:,:,:] = data[..., np.newaxis]

# segment names (for the NG label panel)
import json
seg_props_dir = os.path.join(dst, 'segment_properties'); os.makedirs(seg_props_dir, exist_ok=True)
labels = ho['labels']
json.dump({"@type":"neuroglancer_segment_properties","inline":{
  "ids":[str(i) for i in ids],
  "properties":[{"id":"label","type":"label","values":[labels[i] for i in ids]}]}},
  open(os.path.join(seg_props_dir,'info'),'w'))
info2 = vol.info; info2['segment_properties']='segment_properties'; vol.info=info2; vol.commit_info()

tq = LocalTaskQueue(parallel=1)
tq.insert(tc.create_meshing_tasks(out, mip=0, shape=(256,256,256), mesh_dir='mesh'))
tq.execute()
tq.insert(tc.create_mesh_manifest_tasks(out, mesh_dir='mesh'))
tq.execute()
print("DONE", dst)
print("IDS", ids)
