# Build a native Linux bundle with Docker

[README](../README.md) · [Українська](docker.uk.md)

Requirements on the build host: Git, Docker Engine with **Buildx**, and a POSIX shell. Rust, Cargo, a C compiler and development libraries stay inside the builder. Docker Desktop includes Buildx; Linux Engine packages may ship it separately ([installation](https://docs.docker.com/build/buildx/install/)). Your user must be allowed to use the Docker daemon.

From the checkout:

```sh
./scripts/build-docker.sh
```

The first build downloads dependencies. BuildKit caches Cargo registry, Git and target data; later builds reuse them. An optional argument selects the output directory, including paths with spaces. Files exported to `dist/` belong to the user running the command. No home directory, drives, GUI socket or Docker socket is mounted into the builder; privileged mode is unnecessary.

The bundle contains `disco`, its assets, the desktop installer, documentation and component license notices. The output also includes an SHA-256 file and a report with `ldd` dependencies and the highest referenced glibc version. The exact version comes from Cargo.toml; building does not create tags or change release versions.

```sh
cd dist
sha256sum -c disco-*.tar.gz.sha256
tar -xzf disco-*.tar.gz
cd disco
./disco --version
python3 install.py --binary ./disco
```

Launch **disco** from the menu. Keep the extracted directory in a permanent location: installation points to its executable. You can move it before installation, for example into `~/.local/opt/disco`. To keep an existing development profile, append `--config-dir ~/.config/disco-unstable` to the installation command. The old duplicate disco Unstable launcher is replaced; configuration files are retained.

## Runtime compatibility

The pinned Rust 1.95.0 builder uses Debian 12 (glibc 2.36). Initial support is native Linux amd64; native arm64 can be built on an arm64 Docker host but is not yet verified. Windows/macOS and cross-platform GUI ports are outside this build.

A desktop with Wayland or X11 and working OpenGL/EGL drivers is still required. On Debian 12 / Ubuntu 24.04, runtime packages typically include:

```sh
sudo apt-get install python3 libgcc-s1 libgl1 libegl1 libxkbcommon0 \
  libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libx11-6 libxcursor1 libxi6 libxrandr2 libxinerama1 xdg-utils
```

These are runtime packages, not Rust/dev dependencies. The file dialog uses the desktop's xdg-desktop-portal implementation; the terminal uses the user's installed shell and programs. Trash follows the host's freedesktop desktop environment. Docker does not supply desktop drivers or make the binary compatible with older glibc systems. The generated runtime report is the authoritative glibc requirement for that build.

The `Linux bundle` GitHub Actions workflow exports an archive, checksum and runtime report on pushes to the unstable branch and manual runs. These are CI artifacts, without publishing a release or container image.

## Cache cleanup

`docker buildx du` shows the builder's storage. `docker buildx prune` removes unused build cache after Docker's confirmation. Installed/extracted bundles are independent of the builder and cache. No app settings are stored in the container.
