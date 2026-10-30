#!/usr/bin/env python3
"""Install a stable command and desktop entry for a local disco build."""
import argparse
import json
import os
import shlex
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


def config_directory():
    if os.environ.get('DISCO_CONFIG_DIR'):
        return Path(os.environ['DISCO_CONFIG_DIR']).expanduser().resolve()
    return Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')).expanduser().resolve() / 'disco'


def write_atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.install-next')
    temporary.write_bytes(data)
    os.replace(temporary, path)


def register_desktop_icon(bundle, desktop):
    config = config_directory()
    background = '#101417'
    try:
        import tomllib
        theme_file = config / 'theme.toml'
        if (config / 'themes.toml').is_file():
            catalog = tomllib.loads((config / 'themes.toml').read_text())
            descriptor = catalog.get('themes', {}).get(catalog.get('active'), {})
            relative = Path(descriptor.get('file', ''))
            candidate_file = (config / relative).resolve()
            candidate_file.relative_to(config)
            if relative.parts and not relative.is_absolute() and candidate_file.is_file():
                theme_file = candidate_file
        candidate = tomllib.loads(theme_file.read_text()).get('background', background)
        if isinstance(candidate, str) and len(candidate) == 7 and candidate.startswith('#'):
            int(candidate[1:], 16)
            background = candidate
    except (ImportError, OSError, ValueError, TypeError, AttributeError):
        pass
    def linear(channel):
        value = int(channel, 16) / 255
        return value / 12.92 if value <= .04045 else ((value + .055) / 1.055) ** 2.4
    luminance = sum(weight * linear(background[start:start + 2])
                    for weight, start in zip((.2126, .7152, .0722), (1, 3, 5)))
    dark = 1.05 / (luminance + .05) >= (luminance + .05) / .05
    for tone in ('dark', 'light'):
        write_atomic(config / f'app-icon-{tone}.svg', (bundle / f'assets/disco-{tone}.svg').read_bytes())
    write_atomic(config / 'desktop-integration.toml',
                 ('entry = ' + json.dumps(str(desktop), ensure_ascii=False) + '\n').encode())
    return config / ('app-icon-dark.svg' if dark else 'app-icon-light.svg')


def install(binary=None, config_dir=None):
    bundle = Path(__file__).resolve().parent
    if binary is None:
        built = bundle / 'target/release/disco'
        binary = built if built.is_file() else bundle / 'disco'
    binary = Path(binary).expanduser().resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit('Build disco first: cargo build --release --locked')
    command = Path.home() / '.local/bin/disco'
    if config_dir is None:
        link_atomic(binary, command)
    else:
        config = Path(config_dir).expanduser().resolve()
        os.environ['DISCO_CONFIG_DIR'] = str(config)
        script = '#!/bin/sh\nexec env DISCO_CONFIG_DIR=' + shlex.quote(str(config)) + ' ' + shlex.quote(str(binary)) + ' "$@"\n'
        write_atomic(command, script.encode())
        command.chmod(0o755)
    desktop = Path.home() / '.local/share/applications/ua.disco.desktop'
    desktop.parent.mkdir(parents=True, exist_ok=True)
    icon = register_desktop_icon(bundle, desktop)
    lines = []
    for line in (bundle / 'disco.desktop').read_text().splitlines():
        if line.startswith('Exec='):
            line = 'Exec=' + desktop_command(command)
        elif line.startswith('TryExec='):
            line = 'TryExec=' + desktop_string(command)
        elif line.startswith('Icon='):
            line = 'Icon=' + desktop_string(icon)
        lines.append(line)
    write_atomic(desktop, ('\n'.join(lines) + '\n').encode())
    # Previous development installs used a second product name. Keep one
    # application identity while retaining its independent configuration data.
    legacy = desktop.with_name('ua.disco.unstable.desktop')
    if legacy.is_file() and 'Name=disco Unstable' in legacy.read_text():
        legacy.unlink()
        (command.parent / 'disco-unstable').unlink(missing_ok=True)
    print('Command:', command, '->', binary)
    print('Menu:', desktop)
    print('Keep the project and built executable at these paths.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, help='Use an executable from a custom Cargo target directory')
    parser.add_argument('--config-dir', type=Path, help='Persist a separate configuration directory in the disco launcher')
    args = parser.parse_args()
    install(args.binary, args.config_dir)
