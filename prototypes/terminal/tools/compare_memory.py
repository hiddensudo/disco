#!/usr/bin/env python3
"""Compare only processes launched by this probe; never terminate existing terminals."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def process_table():
    table = {}
    for entry in Path('/proc').iterdir():
        if entry.name.isdigit():
            try:
                fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
                table[int(entry.name)] = int(fields[1])
            except (OSError, ValueError, IndexError):
                pass
    return table

def snapshot(pid):
    parents = process_table()
    pids = {pid}
    while True:
        children = {p for p, parent in parents.items() if parent in pids}
        if children <= pids:
            break
        pids |= children
    result = []
    for target in sorted(pids):
        try:
            values = {}
            for line in Path(f'/proc/{target}/smaps_rollup').read_text().splitlines():
                if ':' in line:
                    key, value = line.split(':', 1)
                    if key in ('Rss', 'Pss', 'Private_Clean', 'Private_Dirty', 'Pss_Anon'):
                        values[key] = int(value.split()[0]) / 1024
            result.append(dict(pid=target, name=Path(f'/proc/{target}/comm').read_text().strip(), **values))
        except OSError:
            pass
    return dict(processes=result, rss_sum_mib=sum(x['Rss'] for x in result), pss_sum_mib=sum(x['Pss'] for x in result), uss_sum_mib=sum(x['Private_Clean'] + x['Private_Dirty'] for x in result), anon_pss_sum_mib=sum(x['Pss_Anon'] for x in result))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    parser.add_argument('--cwd', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()

    base_env = os.environ.copy()
    for key in ('NO_COLOR', 'COLUMNS', 'LINES', 'CLICOLOR', 'FORCE_COLOR'):
        base_env.pop(key, None)
    base_env['HISTFILE'] = '/dev/null'
    proto = [args.binary, '--shell', '/bin/sh', '--folder', args.cwd, '--title', 'disco — memory probe']
    kitty = ['kitty', '--config', 'NONE', '--title', 'kitty — memory probe', '--override', 'initial_window_width=1440', '--override', 'initial_window_height=880', '--override', 'remember_window_size=no', '--override', 'font_family=Noto Sans Mono', '--override', 'font_size=10.5', '--directory', args.cwd, '/bin/sh']
    report = dict(method='Full descendant trees; same /bin/sh, cwd, 1440x880 requested window; Kitty config NONE; 5s warmup + 3 samples; PSS includes a proportional share of shared pages.', rounds=[])
    for round_index in range(2):
        running = {}
        logs = []
        try:
            order = [('prototype', proto), ('kitty', kitty)]
            if round_index:
                order.reverse()
            for name, command in order:
                log = open(f'/tmp/disco-kitty-{round_index}-{name}.log', 'w')
                logs.append(log)
                running[name] = subprocess.Popen(command, cwd=args.cwd, env=base_env, stdout=log, stderr=log, start_new_session=True)
            time.sleep(5)
            samples = {name: [] for name in running}
            for sample in range(3):
                for name, child in running.items():
                    if child.poll() is not None:
                        raise RuntimeError(f'{name} exited: {child.returncode}; inspect /tmp/disco-kitty-{round_index}-{name}.log')
                    samples[name].append(snapshot(child.pid))
                if sample < 2:
                    time.sleep(1)
            medians = {name: {key: statistics.median(s[key] for s in values) for key in ('rss_sum_mib', 'pss_sum_mib', 'uss_sum_mib', 'anon_pss_sum_mib')} for name, values in samples.items()}
            report['rounds'].append(dict(index=round_index + 1, medians=medians, samples=samples))
            print(json.dumps(dict(round=round_index + 1, medians=medians)), flush=True)
        finally:
            # Only the new sessions created above are terminated; user's sessions remain.
            for child in running.values():
                if child.poll() is None:
                    child.terminate()
            for child in running.values():
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
            for log in logs:
                log.close()
    Path(args.output).write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
