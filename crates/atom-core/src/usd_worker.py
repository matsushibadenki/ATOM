"""ATOM OpenUSD adapter: bounded public HF packages and official pxr composition.
No user code, remote SDK plugins, credentials, or viewer process is loaded.
"""
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

MAX_FILE = 64 * 1024 * 1024
MAX_PACKAGE = 256 * 1024 * 1024
MAX_VERTICES = 300000


def fail(code, detail):
    raise ValueError(code + ': ' + detail)


def relative(value):
    if not isinstance(value, str) or len(value) > 1024 or '\\' in value:
        fail('usd_invalid', 'invalid relative path')
    p = PurePosixPath(value)
    if p.is_absolute() or not p.parts or any(x in ('..', '.') for x in p.parts) or ':' in value:
        fail('usd_invalid', 'invalid relative path')
    return str(p)


def repo_id(value):
    if not re.fullmatch(r'[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+', value):
        fail('usd_invalid', 'repository must be namespace/name')
    return value


def fetch(url, limit=MAX_FILE):
    request = urllib.request.Request(url, headers={'User-Agent': 'ATOM-OpenUSD/1.0'})
    with urllib.request.urlopen(request, timeout=25) as response:
        if int(response.headers.get('Content-Length', '0')) > limit:
            fail('usd_budget', 'file size')
        data = response.read(limit + 1)
    if len(data) > limit:
        fail('usd_budget', 'download size')
    return data


def api(path):
    return json.loads(fetch('https://huggingface.co/api/' + path, 8 * 1024 * 1024))


def revision(repo, requested):
    if re.fullmatch(r'[0-9a-f]{40}', requested):
        return requested
    refs = api('datasets/' + repo + '/refs')
    for ref in refs.get('branches', []) + refs.get('tags', []):
        if ref['name'] == requested and re.fullmatch(r'[0-9a-f]{40}', ref['targetCommit']):
            return ref['targetCommit']
    fail('usd_invalid', 'unknown revision')


def catalog(request):
    repo = repo_id(request['repo'])
    rev = revision(repo, request.get('revision', 'main'))
    # The HF root tree is paginated lexicographically; use its HTTP Link cursor.
    url = f'https://huggingface.co/api/datasets/{repo}/tree/{rev}?limit=1000'
    entries = []
    for _ in range(8):
        with urllib.request.urlopen(url, timeout=25) as response:
            data = response.read(4 * 1024 * 1024 + 1)
            if len(data) > 4 * 1024 * 1024:
                fail('usd_budget', 'catalog size')
            rows = json.loads(data)
            link = response.headers.get('Link', '')
        entries.extend(x['path'] for x in rows if x['type'] == 'directory' and not x['path'].startswith('.'))
        match = re.search(r'<([^>]+)>; rel="next"', link)
        if not match:
            return {'repo': repo, 'revision': rev, 'packages': sorted(entries)}
        url = match[1]
        parsed = urllib.parse.urlparse(url)
        if parsed.scheme != 'https' or parsed.netloc != 'huggingface.co':
            fail('usd_invalid', 'catalog pagination')
    fail('usd_budget', 'catalog pages')


def package(request):
    repo = repo_id(request['repo'])
    name = relative(request['package'])
    rev = revision(repo, request.get('revision', 'main'))
    cache = Path(request['cache']).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    base = f'https://huggingface.co/datasets/{repo}/resolve/{rev}/{urllib.parse.quote(name)}/'
    manifest_bytes = fetch(base + 'com.nvidia.simready.packaging.json', 1024 * 1024)
    manifest = json.loads(manifest_bytes)
    if manifest.get('format_version') != '1.0':
        fail('usd_unsupported', 'SimReady package format')
    license_name = str(manifest.get('license', 'unknown'))
    bom_bytes = fetch(base + '.metadata/com.nvidia.simready.packaging.bom.json', 4 * 1024 * 1024)
    expected_bom = next((x.get('hash', {}).get('sha256') for x in manifest.get('metadata', []) if x.get('name') == 'com.nvidia.simready.packaging.bom.json'), None)
    if not expected_bom or hashlib.sha256(bom_bytes).hexdigest() != expected_bom:
        fail('usd_integrity', 'package BOM hash')
    bom = json.loads(bom_bytes)
    roots_bytes = fetch(base + '.metadata/com.nvidia.simready.root_usds.json', 1024 * 1024)
    roots = json.loads(roots_bytes).get('entries', [])
    if len(roots) != 1:
        fail('usd_unsupported', 'select a package with one root USD')
    root_usd = relative(roots[0])
    items = bom.get('items', [])
    if not items or len(items) > 2000:
        fail('usd_budget', 'package file count')
    files = {}
    for item in items:
        path = relative(item['relative_path'])
        if '/.thumbs/' in '/' + path:
            continue
        size = item.get('size')
        digest = item.get('hash', {}).get('sha256')
        if path in files or not isinstance(size, int) or size < 0 or size > MAX_FILE or not re.fullmatch(r'[0-9a-f]{64}', digest or ''):
            fail('usd_invalid', 'BOM entry')
        files[path] = (size, digest)
    if root_usd not in files or sum(x[0] for x in files.values()) > MAX_PACKAGE:
        fail('usd_budget', 'package size or missing root')
    identity = hashlib.sha256((repo + rev + name + hashlib.sha256(manifest_bytes).hexdigest()).encode()).hexdigest()
    target = cache / identity
    # Verify cached bytes on every use; immutable cache entries are never overwritten.
    if not target.exists():
        with tempfile.TemporaryDirectory(dir=cache, prefix='.incoming-') as tmp:
            staging = Path(tmp) / 'package'
            staging.mkdir()
            for path, (size, digest) in files.items():
                content = fetch(base + urllib.parse.quote(path), size)
                if len(content) != size or hashlib.sha256(content).hexdigest() != digest:
                    fail('usd_integrity', 'file hash: ' + path)
                destination = staging / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(content)
            metadata = staging / '.metadata'
            metadata.mkdir(exist_ok=True)
            (metadata / 'com.nvidia.simready.packaging.bom.json').write_bytes(bom_bytes)
            (metadata / 'com.nvidia.simready.root_usds.json').write_bytes(roots_bytes)
            (staging / 'com.nvidia.simready.packaging.json').write_bytes(manifest_bytes)
            if not (metadata / 'LICENSE.txt').exists():
                (metadata / 'LICENSE.txt').write_bytes(fetch(base + '.metadata/LICENSE.txt', 1024 * 1024))
            try:
                staging.rename(target)
            except FileExistsError:
                pass
    for path, (size, digest) in files.items():
        local = (target / path).resolve()
        if not local.is_relative_to(target) or not local.is_file() or local.stat().st_size != size or hashlib.sha256(local.read_bytes()).hexdigest() != digest:
            fail('usd_integrity', 'cached file: ' + path)
    return target / root_usd, target, {
        'kind': 'hugging_face', 'repo': repo, 'revision': rev, 'package': name,
        'entry': root_usd, 'license': license_name, 'author': repo.split('/')[0],
        'source_hash': hashlib.sha256(manifest_bytes).hexdigest(),
        'cache_path': str(target),
    }


def triangulate(points, polygon):
    """Ear-clipping for planar simple polygons, including concave faces."""
    if len(polygon) == 3:
        return [polygon]
    n = [0., 0., 0.]
    for a, b in zip(polygon, polygon[1:] + polygon[:1]):
        p, q = points[a], points[b]
        n[0] += (p[1]-q[1])*(p[2]+q[2]); n[1] += (p[2]-q[2])*(p[0]+q[0]); n[2] += (p[0]-q[0])*(p[1]+q[1])
    drop = max(range(3), key=lambda i: abs(n[i]))
    axes = [i for i in range(3) if i != drop]
    xy = {v: (points[v][axes[0]], points[v][axes[1]]) for v in polygon}
    def cross(a, b, c):
        return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
    area = sum(xy[a][0]*xy[b][1]-xy[b][0]*xy[a][1] for a,b in zip(polygon,polygon[1:]+polygon[:1]))
    if abs(area) < 1e-16:
        return []
    sign = 1 if area > 0 else -1
    remaining, triangles = list(polygon), []
    while len(remaining) > 3:
        for i,b in enumerate(remaining):
            a,c=remaining[i-1],remaining[(i+1)%len(remaining)]
            if cross(xy[a],xy[b],xy[c])*sign <= 1e-14:
                continue
            if any(all(cross(xy[x],xy[y],xy[p])*sign >= -1e-14 for x,y in [(a,b),(b,c),(c,a)]) for p in remaining if p not in (a,b,c)):
                continue
            triangles.append([a,b,c]);remaining.pop(i);break
        else:
            fail('usd_unsupported', 'non-simple or degenerate polygon')
    triangles.append(remaining)
    return triangles


def convert(path, boundary, source):
    try:
        from pxr import Usd, UsdGeom, UsdShade, UsdPhysics, UsdUtils, Sdf
    except ImportError:
        fail('usd_unavailable', 'install usd-core; set ATOM_USD_PYTHON')
    # Validate all layer dependencies before full composition. Packages stay local.
    visited = set()
    def check_layer(file):
        file = file.resolve()
        if not file.is_relative_to(boundary) or not file.is_file():
            fail('usd_dependency', 'layer outside package or missing')
        if file in visited:
            return
        if len(visited) >= 256 or file.stat().st_size > MAX_FILE:
            fail('usd_budget', 'layer count/size')
        visited.add(file)
        layer = Sdf.Layer.FindOrOpen(str(file))
        if not layer:
            fail('usd_invalid', 'invalid layer')
        for ref in layer.GetExternalReferences():
            if not ref:
                continue
            if ':' in ref or '[' in ref:
                fail('usd_unsupported', 'non-local layer reference')
            check_layer(file.parent / ref)
    check_layer(path)
    stage = Usd.Stage.Open(str(path), load=Usd.Stage.LoadAll)
    if not stage:
        fail('usd_invalid', 'cannot compose stage')
    _, _, missing = UsdUtils.ComputeAllDependencies(str(path))
    if missing:
        fail('usd_dependency', 'unresolved dependencies')
    meters = UsdGeom.GetStageMetersPerUnit(stage)
    up = str(UsdGeom.GetStageUpAxis(stage))
    if not math.isfinite(meters) or meters <= 0 or up not in ('Y','Z'):
        fail('usd_invalid', 'stage units/axis')
    time = Usd.TimeCode(0.)
    xforms = UsdGeom.XformCache(time)
    positions, colors, indices = [], [], []
    warnings = {'static_mesh_import', 'convex_collision', 'physics_defaults'}
    prim_paths, masses, friction = [], [], []
    def color_for(prim):
        color = [0.62, 0.58, 0.51]
        if prim.IsA(UsdGeom.Gprim):
            values = UsdGeom.Gprim(prim).GetDisplayColorAttr().Get(time)
            if values:
                color = list(values[0])
                if len(values)>1:warnings.add("display_color_approximation")
        material, _ = UsdShade.MaterialBindingAPI(prim).ComputeBoundMaterial()
        if material:
            for shader_prim in Usd.PrimRange(material.GetPrim()):
                if shader_prim.IsA(UsdShade.Shader):
                    shader = UsdShade.Shader(shader_prim)
                    for name in ('diffuseColor','diffuse_color_constant','base_color'):
                        value = shader.GetInput(name).Get(time)
                        if value is not None and hasattr(value, '__len__') and len(value) == 3:
                            color = list(value)
                    if shader.GetIdAttr().Get() != 'UsdPreviewSurface':
                        warnings.add('mdl_approximation')
                    if any(x.GetTypeName() == Sdf.ValueTypeNames.Asset for x in shader.GetInputs()):
                        warnings.add('textures_not_rendered')
        return [min(1.,max(0.,float(v))) for v in color]
    for prim in Usd.PrimRange.Stage(stage, Usd.TraverseInstanceProxies()):
        if prim.IsA(UsdPhysics.Joint):
            warnings.add('articulation_static')
            continue
        if prim.IsA(UsdGeom.PointInstancer):
            fail('usd_unsupported', 'point instancers require a dedicated adapter')
        if prim.HasAPI(UsdPhysics.MassAPI):
            mass = UsdPhysics.MassAPI(prim).GetMassAttr().Get(time)
            if mass and math.isfinite(mass) and mass > 0:
                masses.append(float(mass)*UsdPhysics.GetStageKilogramsPerUnit(stage))
        if not prim.IsA(UsdGeom.Mesh) and not prim.IsA(UsdGeom.Cube):
            if prim.IsA(UsdGeom.Boundable) and not prim.IsA(UsdGeom.Camera):
                warnings.add('unsupported_primitives')
            continue
        image = UsdGeom.Imageable(prim)
        if image.ComputeVisibility(time) == UsdGeom.Tokens.invisible or image.ComputePurpose() in (UsdGeom.Tokens.proxy, UsdGeom.Tokens.guide):
            continue
        if prim.IsA(UsdGeom.Mesh):
            mesh = UsdGeom.Mesh(prim)
            points = mesh.GetPointsAttr().Get(time)
            counts = list(mesh.GetFaceVertexCountsAttr().Get(time) or [])
            faces = list(mesh.GetFaceVertexIndicesAttr().Get(time) or [])
            holes = set(mesh.GetHoleIndicesAttr().Get(time) or [])
            left = mesh.GetOrientationAttr().Get() == UsdGeom.Tokens.leftHanded
            if mesh.GetSubdivisionSchemeAttr().Get() != UsdGeom.Tokens.none:
                warnings.add('subdivision_control_mesh')
        else:
            size = UsdGeom.Cube(prim).GetSizeAttr().Get(time)/2.
            points = [(x*size,y*size,z*size) for x,y,z in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
            counts=[4]*6;faces=[0,3,2,1,4,5,6,7,0,1,5,4,3,7,6,2,1,2,6,5,0,4,7,3];holes=set();left=False
        if points is None or len(points)>MAX_VERTICES or len(counts)>MAX_VERTICES or sum(counts)!=len(faces) or any(n<3 or n>1024 for n in counts) or any(i<0 or i>=len(points) for i in faces):
            fail('usd_invalid', 'mesh topology')
        from pxr import Gf
        matrix=xforms.GetLocalToWorldTransform(prim)
        world=[]
        for point in points:
            p=matrix.Transform(Gf.Vec3d(*point));p=[float(v)*meters for v in p]
            world.append([p[0],p[2],-p[1]] if up=='Z' else p)
        reverse = left != (matrix.GetDeterminant()<0.)
        face_colors={};base_color=color_for(prim)
        if prim.IsA(UsdGeom.Mesh):
            for subset in UsdGeom.Subset.GetAllGeomSubsets(UsdGeom.Mesh(prim)):
                if subset.GetElementTypeAttr().Get() == UsdGeom.Tokens.face:
                    c=color_for(subset.GetPrim())
                    for face in subset.GetIndicesAttr().Get(time) or []: face_colors[int(face)]=c
        offset=0
        for face,n in enumerate(counts):
            polygon=faces[offset:offset+n];offset+=n
            if face in holes:continue
            for triangle in triangulate(world,polygon):
                if reverse: triangle=triangle[::-1]
                if len(positions)+3>MAX_VERTICES:fail('usd_budget','triangle count')
                for i in triangle:
                    indices.append(len(positions));positions.append(list(world[i]));colors.append(face_colors.get(face,base_color))
        prim_paths.append(str(prim.GetPath()))
    if not positions:
        fail('usd_unsupported','no visible supported meshes')
    if any(not math.isfinite(v) or abs(v)>10000 for p in positions for v in p):
        fail('usd_invalid','non-finite or oversized geometry')
    minimum=[min(p[i] for p in positions) for i in range(3)];maximum=[max(p[i] for p in positions) for i in range(3)]
    scale=[max(maximum[i]-minimum[i],0.001) for i in range(3)];center=[(minimum[i]+maximum[i])/2. for i in range(3)]
    for p in positions:
        for i in range(3):p[i]=(p[i]-center[i])/scale[i]
    metadata_mass=stage.GetRootLayer().customLayerData.get('SimReady_Metadata',{}).get('mass')
    mass=masses[0] if len(masses)==1 else None
    if mass is None and isinstance(metadata_mass,(int,float)) and math.isfinite(metadata_mass) and metadata_mass>0:
        mass=float(metadata_mass)
    source.update({'up_axis':up,'meters_per_unit':meters,'prim_paths':prim_paths[:256],'sdk_version':'.'.join(map(str,Usd.GetVersion()))})
    return {'name': path.stem[:64], 'source':source, 'mesh':{'positions':positions,'colors':colors,'indices':indices},'position':center,'scale':scale,'mass':mass,'warnings':sorted(warnings)}


def main(request):
    if request['mode']=='catalog':return catalog(request)
    if request['mode']=='hub':
        path, boundary, source=package(request)
    else:
        path=Path(request['path']).resolve(strict=True);boundary=path.parent
        if path.suffix.lower() not in ('.usd','.usda','.usdc'):
            fail('usd_unsupported','supported: USD, USDA, USDC; extract USDZ first')
        source={'kind':'local','repo':'','revision':'','package':'','entry':path.name,'license':'unknown','author':'','source_hash':hashlib.sha256(path.read_bytes()).hexdigest(),'cache_path':''}
        # Local composition and copies are validated with the same dependency boundary.
        from pxr import UsdUtils
        # Dependency copying happens after validation below, avoiding unresolved or partial imports.
    result=convert(path,boundary,source)
    if source['kind']=='local':
        from pxr import UsdUtils
        layers,assets,missing=UsdUtils.ComputeAllDependencies(str(path))
        files={Path(l.realPath).resolve() for l in layers if l.realPath} | {Path(a).resolve() for a in assets}
        if missing or len(files)>2000 or any(not f.is_relative_to(boundary) or not f.is_file() or f.stat().st_size>MAX_FILE for f in files) or sum(f.stat().st_size for f in files)>MAX_PACKAGE:
            fail('usd_dependency','local dependency boundary/budget')
        cache=Path(request['cache']).resolve();cache.mkdir(parents=True,exist_ok=True)
        digest=hashlib.sha256()
        for f in sorted(files):digest.update(str(f.relative_to(boundary)).encode());digest.update(hashlib.sha256(f.read_bytes()).digest())
        destination=cache/digest.hexdigest();destination.mkdir(exist_ok=True)
        for f in files:
            target=destination/f.relative_to(boundary);target.parent.mkdir(parents=True,exist_ok=True)
            if f.resolve()!=target.resolve():shutil.copyfile(f,target)
        result['source']['source_hash']=digest.hexdigest();result['source']['cache_path']=str(destination)
    return result


if __name__=='__main__':
    try:
        request=json.loads(Path(sys.argv[1]).read_text())
        result={'ok':main(request)}
    except Exception as error:
        text=str(error)
        codes=['usd_unavailable','usd_invalid','usd_budget','usd_unsupported','usd_integrity','usd_dependency']
        code=next((c for c in codes if text.startswith(c+':')),'usd_unavailable' if isinstance(error,ImportError) else 'usd_network' if isinstance(error,(urllib.error.URLError,TimeoutError)) else 'usd_invalid')
        result={'error':code,'detail':text[:500]}
    Path(sys.argv[2]).write_text(json.dumps(result,allow_nan=False))
