FROM rust:1.97-bookworm

ENV DEBIAN_FRONTEND=noninteractive
ENV RUST_BACKTRACE=1
ENV CARGO_TERM_COLOR=always

# Install all build, X11, desktop apps, accessibility, media, packaging, and QA utilities
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config curl git ca-certificates \
    libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev \
    debhelper dpkg-dev libarchive-tools \
    xvfb xdotool wmctrl x11-utils x11-xserver-utils scrot imagemagick \
    openbox dbus-x11 libnotify-bin \
    pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator supertux \
    chocolate-doom freedoom feh gimp inkscape vlc libreoffice-writer libreoffice-calc thunderbird \
    pavucontrol pulseaudio pulseaudio-utils pipewire pipewire-pulse wireplumber \
    blueman upower xfce4-power-manager xfce4-settings lxappearance arandr \
    network-manager network-manager-gnome \
    adwaita-icon-theme librsvg2-common fonts-liberation fonts-dejavu-core \
    at-spi2-core gir1.2-atspi-2.0 gir1.2-gtk-3.0 python3-gi python3-requests \
    python3-psutil orca speech-dispatcher appmenu-gtk3-module \
    locales sudo \
    && rm -rf /var/lib/apt/lists/*

# Generate locales required for internationalization and accessibility QA
RUN echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen && \
    echo "fr_FR.UTF-8 UTF-8" >> /etc/locale.gen && \
    echo "de_DE.UTF-8 UTF-8" >> /etc/locale.gen && \
    echo "ar_SA.UTF-8 UTF-8" >> /etc/locale.gen && \
    echo "he_IL.UTF-8 UTF-8" >> /etc/locale.gen && \
    locale-gen

# Pre-install clippy and rustfmt
RUN rustup component add clippy rustfmt

# Create a non-root test user for permissions/security tests
RUN useradd -m -s /bin/bash tester && \
    echo "tester ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

WORKDIR /workspace
