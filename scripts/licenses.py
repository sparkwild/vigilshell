"""Build offline notices from locked dependencies and reviewed upstream supplements."""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / 'resources/licenses'

def npm_license_paths(packages, bundled, stylesheets=()):
    """Collect locked runtime and renderer-bundled dependencies, including their peers.

    Renderer packages live in devDependencies to avoid duplicate app.asar copies,
    but their transitive code and font licenses still belong in the distribution.
    """
    root = packages['']
    declared = root.get('dependencies', {}) | root.get('devDependencies', {})
    for name in [*bundled, *stylesheets]:
        if name not in declared:
            raise RuntimeError('Undeclared bundled dependency: ' + name)

    def resolve(parent, name, optional=False):
        while True:
            candidate = (parent + '/' if parent else '') + 'node_modules/' + name
            if candidate in packages:
                return candidate
            if not parent:
                if optional: return None
                raise RuntimeError('Missing locked dependency: ' + name)
            parent = parent.rsplit('/node_modules/', 1)[0] if '/node_modules/' in parent else ''

    pending = [resolve('', name) for name in set(root.get('dependencies', {})) | set(bundled)]
    seen = set()
    while pending:
        path = pending.pop()
        if path is None or path in seen: continue
        seen.add(path)
        package = packages[path]
        optional = package.get('optionalDependencies', {})
        peers = package.get('peerDependencies', {})
        peer_meta = package.get('peerDependenciesMeta', {})
        dependencies = package.get('dependencies', {}) | optional | peers
        for name in dependencies:
            may_be_absent = name in optional or peer_meta.get(name, {}).get('optional', False)
            pending.append(resolve(path, name, may_be_absent))
    # Stylesheet-only imports ship the package's CSS, not its CLI dependency tree.
    return seen | {resolve('', name) for name in stylesheets}

def collect():
    rows = []
    supplements = json.loads((OUT / 'upstream.json').read_text()) if (OUT / 'upstream.json').exists() else {}
    def add(name, version, source, license_name, base, files=None):
        files = files if files is not None else sorted(p for p in base.iterdir() if p.is_file() and re.match(r'^(licen[sc]e|notice|copying|copyright)([.\-_]|$)', p.name, re.I))
        documents = [{'file': p.name, 'text': p.read_text(), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]
        key = name + '@' + version
        if not documents:
            if key not in supplements:
                if not license_name:
                    raise RuntimeError('Missing license declaration: ' + key)
                documents = [{'file': 'PACKAGE_LICENSE_DECLARATION', 'text': 'License declared by the package: ' + str(license_name) + '. No license text was bundled; the package declaration and source are retained without adding a copyright attribution.'}]
            else:
                documents = supplements[key]
        for document in documents:
            if 'source' in document and 'sha256' in document and hashlib.sha256(document['text'].encode()).hexdigest() != document['sha256']:
                raise RuntimeError('License content hash mismatch: ' + key)
        rows.append({'name': name, 'version': version, 'source': source, 'license': license_name, 'documents': documents})
    lock = json.loads((ROOT / 'package-lock.json').read_text())
    bundled = json.loads((ROOT / 'scripts/bundled-dependencies.json').read_text())
    for path in sorted(npm_license_paths(lock['packages'], bundled['packages'], bundled['stylesheets'])):
        pkg = lock['packages'][path]
        base = ROOT / path
        manifest = json.loads((base / 'package.json').read_text())
        if manifest['version'] != pkg['version']: raise RuntimeError('Installed version differs: ' + path)
        add(manifest['name'], pkg['version'], manifest.get('repository', pkg.get('resolved')), manifest.get('license'), base)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', 'aarch64-apple-darwin'], cwd=ROOT))
    edges = {n['id']: [d['pkg'] for d in n['deps'] if any(k['kind'] != 'dev' for k in d['dep_kinds'])] for n in metadata['resolve']['nodes']}
    seen = set(); todo = list(metadata['workspace_members'])
    while todo:
        node = todo.pop()
        if node in seen: continue
        seen.add(node); todo.extend(edges.get(node, []))
    for pkg in metadata['packages']:
        if pkg['id'] in seen and pkg['source']:
            add(pkg['name'], pkg['version'], pkg['repository'], pkg['license'], pathlib.Path(pkg['manifest_path']).parent)
    for name in ['electron']:
        base = ROOT / 'node_modules' / name
        pkg = json.loads((base / 'package.json').read_text())
        add(name, pkg['version'], pkg.get('repository'), pkg['license'], base)
    for p in sorted(OUT.glob('*.txt')):
        if p.name in ['THIRD_PARTY_NOTICES.txt', 'electron-LICENSE.txt']: continue
        add(p.stem, 'bundled', 'resources/licenses/' + p.name, 'See original notice', OUT, [p])
    return sorted(rows, key=lambda r: (r['name'], r['version']))

def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--check', action='store_true'); args = parser.parse_args()
    rows = collect()
    text = 'VigilShell — Apache-2.0\nCopyright 2026 sparkwild\n\n' + (ROOT / 'LICENSE').read_text()
    text += '\n\nThird-party software retains its original licenses.\n'
    for row in rows:
        text += '\n\n' + '=' * 72 + '\n' + row['name'] + ' ' + row['version'] + '\n' + str(row['source']) + '\n'
        for doc in row['documents']: text += '\n' + doc['file'] + '\n' + doc['text']
    index = [{**row, 'documents': [{k:v for k,v in doc.items() if k != 'text'} for doc in row['documents']]} for row in rows]
    outputs = {
        'THIRD_PARTY_NOTICES.txt': text,
        'index.json': json.dumps(index, ensure_ascii=False, indent=2)+'\n',
        'electron-LICENSE.txt': (ROOT/'node_modules/electron/dist/LICENSE').read_text(),
    }
    for name, content in outputs.items():
        path = OUT / name
        if args.check:
            if not path.exists() or path.read_text() != content: raise RuntimeError('Stale license artifact: ' + name)
        else: path.write_text(content)
    declarations_only = [row['name'] + '@' + row['version'] for row in rows if any(d['file'] == 'PACKAGE_LICENSE_DECLARATION' for d in row['documents'])]
    print('Indexed', len(rows), 'third-party entries')
    if declarations_only:
        print('Notice: license declared without bundled text (non-blocking): ' + ', '.join(declarations_only), file=sys.stderr)
if __name__ == '__main__': main()
