#!/usr/bin/env python3
"""Run the actual GPUI scenes against new, owned Unix-only Redis fixtures."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time
from datetime import datetime

ROOT = Path(__file__).resolve().parents[1]
MARKER = b'rust-rdm-ui-owned-v1\n'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def request(folder, *args, db=0):
    assert (folder / 'RDM-OWNED-REDIS').read_bytes() == MARKER
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(8)
        client.connect(str(folder / 'redis.sock'))
        stream = client.makefile('rb')

        def send(items):
            values = [item if isinstance(item, bytes) else str(item).encode() for item in items]
            client.sendall(b'*' + str(len(values)).encode() + b'\r\n' + b''.join(
                b'$' + str(len(item)).encode() + b'\r\n' + item + b'\r\n' for item in values))

        def read():
            line = stream.readline()
            tag, value = line[:1], line[1:-2]
            if tag == b'+':
                return value
            if tag == b':':
                return int(value)
            if tag == b'$':
                length = int(value)
                if length < 0:
                    return None
                data = stream.read(length)
                assert stream.read(2) == b'\r\n'
                return data
            if tag == b'*':
                return [read() for _ in range(int(value))]
            raise RuntimeError(line)

        send(['SELECT', db])
        assert read() == b'OK'
        send(args)
        return read()


def snapshot(folder):
    encode = lambda value: base64.b64encode(value).decode()
    return {
        'keys': [{'key': encode(key), 'dump': encode(request(folder, 'DUMP', key)),
                  'type': request(folder, 'TYPE', key).decode(), 'pttl': request(folder, 'PTTL', key)}
                 for key in sorted(request(folder, 'KEYS', '*'))],
        'otherDBSizes': [request(folder, 'DBSIZE', db=db) for db in range(1, 16)],
        'aclUsers': [value.decode() for value in request(folder, 'ACL', 'USERS')],
        'files': {name: encode((folder / name).read_bytes()) for name in ['connections.yaml', 'preferences.yaml']}
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True)
    parser.add_argument('--scenes', help='Comma-separated exact scene names; omit for every registered scene')
    parser.add_argument('--prefix', help='Run the registered scenes with this prefix')
    parser.add_argument('--json-module', default='/opt/homebrew/opt/redis/lib/redis/modules/rejson.so')
    args = parser.parse_args()
    output = (ROOT / args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    images = output / 'ui'
    images.mkdir()
    block = (ROOT / 'src/ui/qa.rs').read_text().split('let snapshots = [', 1)[1].split('];', 1)[0]
    registered = re.findall(r'^\s*"([^"\n]+)"\s*,?\s*$', block, re.M)
    assert not (args.scenes and args.prefix), 'Choose --scenes or --prefix'
    selected = args.scenes.split(',') if args.scenes else [name for name in registered if not args.prefix or name.startswith(args.prefix)]
    assert selected and set(selected) <= set(registered)
    expected = [name for name in registered if name in selected]
    paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
    frozen = [{'path': path, 'sha256': sha(ROOT / path)} for path in sorted(set(paths)) if path and (ROOT / path).is_file()]
    write(output / 'inputs.json', {'inputs': frozen, 'sceneNames': expected, 'registeredSceneCount': len(registered)})
    executable = ROOT / 'target/debug/examples/render_qa'
    report = {'startedAt': datetime.now().astimezone().isoformat(), 'expectedScenes': expected,
              'registeredSceneCount': len(registered), 'qaExecutableSHA256': sha(executable), 'passed': False}
    fixture_root = Path(tempfile.mkdtemp(prefix='redisdock-ui-', dir='/private/tmp'))
    processes, fixtures = [], []
    try:
        for kind in ['ordinary', 'json']:
            folder = fixture_root / kind
            folder.mkdir()
            (folder / 'RDM-OWNED-REDIS').write_bytes(MARKER)
            command = ['redis-server', '--port', '0', '--save', '', '--appendonly', 'no',
                       '--unixsocket', str(folder / 'redis.sock'), '--unixsocketperm', '700',
                       '--dir', str(folder), '--logfile', str(folder / 'redis.log')]
            if kind == 'json':
                command += ['--loadmodule', args.json_module]
            child = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            processes.append(child)
            for attempt in range(100):
                if child.poll() is not None:
                    raise RuntimeError((folder / 'redis.log').read_text())
                try:
                    assert request(folder, 'PING') == b'PONG'
                    break
                except (OSError, RuntimeError):
                    time.sleep(.05)
            else:
                raise RuntimeError('Disposable Redis did not become ready')
            env = dict(os.environ, REDISDOCK_DATA_DIR=str(folder))
            subprocess.run([str(ROOT / 'target/debug/examples/seed_demo'), str(folder), '--render-qa'],
                           cwd=ROOT, env=env, check=True)
            if kind == 'json':
                request(folder, 'DEL', 'aahash')
                request(folder, 'JSON.SET', 'aahash', '$', '{"name":"中文","items":[1,true]}')
            baseline = snapshot(folder)
            assert len(baseline['keys']) == 19 and all(row['pttl'] == -1 for row in baseline['keys'])
            write(output / f'{kind}-baseline.json', baseline)
            fixtures.append({'kind': kind, 'root': str(folder), 'pid': child.pid, 'baseline': baseline})
        env = dict(os.environ, REDISDOCK_DATA_DIR=fixtures[0]['root'],
                   REDISDOCK_JSON_QA_DIR=fixtures[1]['root'], REDISDOCK_QA_STATUS_CHART_TRACE='1', RUST_BACKTRACE='1')
        for key in ['REDISDOCK_QA_SCENES', 'REDISDOCK_QA_SCENE', 'REDISDOCK_QA_PREFIX']:
            env.pop(key, None)
        if args.scenes or args.prefix:
            env['REDISDOCK_QA_SCENES'] = ','.join(expected)
        log = output / 'ui.log'
        report['fixtures'] = [{k: value for k, value in record.items() if k != 'baseline'} for record in fixtures]
        report['log'] = str(log)
        write(output / 'execution.json', report)
        with log.open('w') as stream:
            child = subprocess.Popen([str(executable), fixtures[0]['root'], str(images)], cwd=ROOT,
                                     env=env, stdout=stream, stderr=subprocess.STDOUT)
            processes.append(child)
            previous, last = -1, 0
            while child.poll() is None:
                names = re.findall(r'^' + re.escape(str(images)) + r'/([^\n/]+)\.png$', log.read_text(), re.M)
                if len(names) >= previous + 25 or time.monotonic() - last > 45:
                    print(f'UI QA: {len(names)}/{len(expected)} rendered; latest={names[-1] if names else "startup"}', flush=True)
                    previous, last = len(names), time.monotonic()
                time.sleep(1)
        content = log.read_text()
        names = re.findall(r'^' + re.escape(str(images)) + r'/([^\n/]+)\.png$', content, re.M)
        changed = [row['path'] for row in frozen if not (ROOT / row['path']).exists() or sha(ROOT / row['path']) != row['sha256']]
        report.update(exitCode=child.returncode, scenes=names, renderedScenes=len(names),
                      sceneNamesMatchCurrentSource=names == expected, sourceInputsChanged=changed,
                      logSHA256=sha(log), retainedLegacyNegatives=re.findall(r'^QA RETAINED NEGATIVE:.*', content, re.M))
        report['passed'] = child.returncode == 0 and names == expected and not changed and 'panicked at' not in content
        for record in fixtures:
            folder = Path(record['root'])
            observed = snapshot(folder)
            write(output / f'{record["kind"]}-after.json', observed)
            if report['passed']:
                assert {row['key'] for row in observed['keys']} == {row['key'] for row in record['baseline']['keys']}
                assert observed['aclUsers'] == ['default'] and observed['otherDBSizes'] == [0] * 15
                assert observed['files']['connections.yaml'] == record['baseline']['files']['connections.yaml']
                assert request(folder, 'GET', 'aastring') == b'Hello Rust + GPUI'
            for row in record['baseline']['keys']:
                request(folder, 'RESTORE', base64.b64decode(row['key']), 0, base64.b64decode(row['dump']), 'REPLACE')
            for name, value in record['baseline']['files'].items():
                (folder / name).write_bytes(base64.b64decode(value))
            restored = snapshot(folder)
            write(output / f'{record["kind"]}-restored.json', restored)
            assert restored['keys'] == record['baseline']['keys']
        report['fixtureBaselineRestoredByHarness'] = True
    except BaseException as error:
        report.update(passed=False, error=f'{type(error).__name__}: {error}')
        raise
    finally:
        for child in reversed(processes):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        report['ownedProcessesExited'] = all(child.poll() is not None for child in processes)
        report['completedAt'] = datetime.now().astimezone().isoformat()
        write(output / 'execution.json', report)
        print(json.dumps({key: report.get(key) for key in ['passed', 'exitCode', 'renderedScenes', 'error', 'ownedProcessesExited']}, ensure_ascii=False), flush=True)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
