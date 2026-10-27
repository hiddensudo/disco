#!/usr/bin/env python3
"""Install a stable command and desktop entry for a local disco build."""
import argparse
import os
from pathlib import Path


def link_atomic(target, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + '.install-next')
    temporary.unlink(missing_ok=True)
    temporary.symlink_to(target)
    os.replace(temporary, destination)


def desktop_string(value):
    return str(value).replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r')


def desktop_command(value):
    # The desktop Exec field parses quoted arguments, then string escapes.
    quoted = str(value)
    for char in ('\\', '"', '`', '$'):
        quoted = quoted.replace(char, '\\' + char)
    return desktop_string('"' + quoted + '"')


def install(binary=None):
    bundle = Path(__file__).resolve().parent
    if binary is None:
        built = bundle / 'target/release/disco'
        binary = built if built.is_file() else bundle / 'disco'
    binary = Path(binary).expanduser().resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit('Build disco first: cargo build --release --locked')
    command = Path.home() / '.local/bin/disco'
    link_atomic(binary, command)
    desktop = Path.home() / '.local/share/applications/ua.disco.desktop'
    desktop.parent.mkdir(parents=True, exist_ok=True)
    lines = []
    for line in (bundle / 'disco.desktop').read_text().splitlines():
        if line.startswith('Exec='):
            line = 'Exec=' + desktop_command(command)
        elif line.startswith('TryExec='):
            line = 'TryExec=' + desktop_string(command)
        elif line.startswith('Icon='):
            line = 'Icon=' + desktop_string(bundle / 'assets/disco.svg')
        lines.append(line)
    desktop.write_text('\n'.join(lines) + '\n')
    print('Command:', command, '->', binary)
    print('Menu:', desktop)
    print('Keep the project and built executable at these paths.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, help='Use an executable from a custom Cargo target directory')
    install(parser.parse_args().binary)
