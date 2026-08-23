# Real-desktop environment for testing that GitSpark's OS handoffs (open in
# external editor, reveal in file manager, open with default program, open
# URL) actually complete — not just that GitSpark calls the right command,
# which is all the existing bare-Xvfb e2e-native job can prove.
#
# The repo itself is NOT baked in here — it's bind-mounted at `docker run`
# time (see the "e2e-gui-handoff-linux" CI job and the usage note below), so
# rebuilding this image is only needed when the environment itself changes,
# not on every source change.
#
# Local usage, from the repo root:
#   docker build -t gitspark-e2e-linux-desktop docker/e2e-linux-desktop
#   docker run --rm -v "$PWD":/workspace gitspark-e2e-linux-desktop
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Same Linux build deps as the e2e-native CI job, plus a real desktop stack:
# fluxbox (a window manager, so xdg-open has a desktop environment to
# resolve against — bare Xvfb has none) and the xdg/mime tooling needed to
# register and exercise a real default-application handoff.
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      git \
      libasound2-dev \
      libclang-dev \
      libgl1-mesa-dri \
      libgtk-3-dev \
      libssl-dev \
      libwayland-dev \
      libxcb-render0-dev \
      libxcb-shape0-dev \
      libxcb-xfixes0-dev \
      libxkbcommon-dev \
      libxkbcommon-x11-dev \
      mesa-vulkan-drivers \
      pkg-config \
      xvfb \
      fluxbox \
      xdg-utils \
      desktop-file-utils \
      shared-mime-info \
      wmctrl \
      x11-utils \
      file \
      build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable

# Register the stub app (docker/e2e-linux-desktop/stub-app.sh) as the real
# default handler for text/plain, http(s) URLs, and plain directories, so
# the e2e suite exercises genuine xdg-open resolution rather than a made-up
# shortcut. text/plain rather than a custom fixture-only mime type: xdg-mime
# resolves a file's type by sniffing its actual content (via `file`), not
# by extension — a custom glob for a made-up extension loses to the
# generic "looks like text" magic rule for any plain-text fixture file, so
# claiming the real text/plain default is what actually routes reliably.
COPY stub-app.sh /usr/local/bin/gitspark-stub.sh
RUN chmod +x /usr/local/bin/gitspark-stub.sh
COPY stub-app.desktop /usr/share/applications/gitspark-stub.desktop
RUN printf '[Default Applications]\ntext/plain=gitspark-stub.desktop\nx-scheme-handler/http=gitspark-stub.desktop\nx-scheme-handler/https=gitspark-stub.desktop\ninode/directory=gitspark-stub.desktop\n' \
      > /usr/share/applications/mimeapps.list \
    && update-desktop-database /usr/share/applications

ENV DISPLAY=:99 \
    XDG_CURRENT_DESKTOP=X-Generic \
    GITSPARK_GUI_MARKER_FILE=/tmp/gitspark-gui-marker.log

WORKDIR /workspace
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
