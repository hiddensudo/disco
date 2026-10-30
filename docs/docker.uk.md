# Нативна Linux-збірка через Docker

[README](../README.uk.md) · [English](docker.md)

На комп’ютері для збірки потрібні Git, Docker Engine з **Buildx** та POSIX shell. Rust, Cargo, C-компілятор і dev-бібліотеки залишаються всередині builder-контейнера. Docker Desktop містить Buildx; для Linux Engine це може бути окремий пакет ([встановлення](https://docs.docker.com/build/buildx/install/)). Користувач повинен мати доступ до Docker daemon.

У директорії репозиторію:

```sh
./scripts/build-docker.sh
```

Перша збірка завантажує залежності. BuildKit кешує Cargo registry, Git і target; наступні збірки використовують кеш. Необов’язковий аргумент задає вихідну директорію, зокрема шлях із пробілами. Експортовані файли в `dist/` належать користувачеві, що запустив команду. Домашню папку, диски, GUI-сокети чи Docker-сокет у builder не монтуємо; privileged не потрібен.

Архів містить `disco`, assets, desktop-інсталятор, документацію та ліцензійні повідомлення компонентів. Поруч є SHA-256 і звіт із `ldd`-залежностями та найбільшою потрібною версією glibc. Версія береться з Cargo.toml; збірка не створює тегів і не змінює номер релізу.

```sh
cd dist
sha256sum -c disco-*.tar.gz.sha256
tar -xzf disco-*.tar.gz
cd disco
./disco --version
python3 install.py --binary ./disco
```

Запускай **disco** з меню. Залиши розпаковану директорію на постійному місці: ярлик посилається на її executable. Перед встановленням її можна перенести, наприклад у `~/.local/opt/disco`. Щоб зберегти чинний development-профіль, додай до команди встановлення `--config-dir ~/.config/disco-unstable`. Старий дубль ярлика disco Unstable замінюється; налаштування зберігаються.

## Сумісність запуску

Зафіксований Rust 1.95.0 builder використовує Debian 12 (glibc 2.36). Початково підтримується нативний Linux amd64. Нативний arm64 можна зібрати на arm64 Docker-хості, але він ще не перевірений. Windows/macOS та порт GUI на інші платформи не входять у цю задачу.

Для запуску все ще потрібні Wayland або X11, OpenGL/EGL і робочі драйвери відеокарти. Для Debian 12 / Ubuntu 24.04 зазвичай потрібні такі runtime-пакети:

```sh
sudo apt-get install python3 libgcc-s1 libgl1 libegl1 libxkbcommon0 \
  libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libx11-6 libxcursor1 libxi6 libxrandr2 libxinerama1 xdg-utils
```

Це пакети для запуску, а не Rust/dev-залежності. Діалог файлів використовує desktop-реалізацію xdg-desktop-portal; термінал — встановлений shell і програми користувача. Кошик використовує freedesktop-оточення хоста. Docker не постачає драйверів робочого столу й не забезпечує сумісності зі старішою glibc. Точну вимогу glibc конкретної збірки містить згенерований runtime-звіт.

GitHub Actions workflow `Linux bundle` після пушу unstable-гілки або ручного запуску експортує архів, checksum та runtime-звіт як CI-артефакти. Реліз чи контейнерний образ не публікується.

## Очищення кешу

`docker buildx du` показує використане builder-місце. `docker buildx prune` видаляє невикористаний кеш після підтвердження Docker. Встановлена/розпакована програма не залежить від builder чи кешу. Налаштування застосунка в контейнері не зберігаються.
