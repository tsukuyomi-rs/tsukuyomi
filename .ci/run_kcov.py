#!/usr/bin/env python3

import itertools
import json
import subprocess
import sys
import os

def is_test_artifact(obj):
    profile = obj.get("profile")
    if not profile:
        return False
    return profile.get("test", False)

def extract_filenames(obj):
    return obj.get("filenames", [])

def run_command(cmd, **kwargs):
    print('[dbg] running:', ' '.join(map(lambda s: '"{}"'.format(s), cmd)), file=sys.stderr)
    return subprocess.run(cmd, **kwargs)

def collect_artifacts(*args):
    command = ["cargo", "test", "--no-run", "--message-format=json"] + list(*args)
    output = run_command(command, stdout = subprocess.PIPE)

    return \
        itertools.chain.from_iterable(
            map(extract_filenames,
                filter(is_test_artifact, \
                    map(json.loads, \
                        filter(lambda x: x != '', \
                            str(output.stdout, 'utf-8').split('\n'))))))

def get_sysroot():
    output = subprocess.run(
        ["rustc", "--print", "sysroot"],
        stdout = subprocess.PIPE,
    )
    return str(output.stdout, encoding = 'utf-8').strip()

def get_target_directory():
    output = subprocess.run(
        ["cargo", "metadata", "--format-version=1"],
        stdout = subprocess.PIPE,
    )
    metadata = json.loads(str(output.stdout, encoding = 'utf-8'))
    return metadata.get("target_directory", None)

if __name__ == '__main__':
    os.environ['RUSTFLAGS'] = '-C link-dead-code'
    os.environ['LD_LIBRARY_PATH'] = os.environ.get('LD_LIBRARY_PATH', '') + ':' + os.path.join(get_sysroot(), 'lib')

    kcov_out = os.path.join(get_target_directory(), 'cov')
    print("[dbg] kcov_out =", kcov_out, file = sys.stderr)

    for artifact in collect_artifacts(sys.argv[1:]):
        kcov_cmd = ["kcov", "--exclude-pattern=/.cargo", kcov_out, artifact]
        run_command(kcov_cmd)
