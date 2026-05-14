# yggdrasil-godot — POC Godot 4.5 (YG-31..YG-35)
#
# Stage 1: instala Godot 4.5 headless + export templates, exporta os dois targets.
# Stage 2: imagem slim que roda o binário headless. Web target fica disponível
# em /app/web para um servidor estático (não exposto por esta imagem; o
# yggdrasil-web Rust serve os assets quando o POC for promovido).
#
# Porta exposta: 3031 (Godot headless server, separado da :3030 do Rust).

ARG GODOT_VERSION=4.5-stable

# --- Stage 1: builder ---------------------------------------------------------
FROM debian:trixie-slim AS builder

ARG GODOT_VERSION

ENV DEBIAN_FRONTEND=noninteractive \
    GODOT_VERSION=${GODOT_VERSION}

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        unzip \
        bash \
    && rm -rf /var/lib/apt/lists/*

# Godot 4.5 headless binary.
RUN curl -fsSL -o /tmp/godot.zip \
        "https://github.com/godotengine/godot/releases/download/${GODOT_VERSION}/Godot_v${GODOT_VERSION}_linux.x86_64.zip" \
    && unzip -q /tmp/godot.zip -d /tmp/godot \
    && mv /tmp/godot/Godot_v${GODOT_VERSION}_linux.x86_64 /usr/local/bin/godot \
    && chmod +x /usr/local/bin/godot \
    && rm -rf /tmp/godot /tmp/godot.zip

# Export templates 4.5 — necessários para `--export-release`.
RUN curl -fsSL -o /tmp/templates.tpz \
        "https://github.com/godotengine/godot/releases/download/${GODOT_VERSION}/Godot_v${GODOT_VERSION}_export_templates.tpz" \
    && mkdir -p /root/.local/share/godot/export_templates/${GODOT_VERSION%-*}.stable \
    && unzip -q /tmp/templates.tpz -d /tmp/templates \
    && mv /tmp/templates/templates/* /root/.local/share/godot/export_templates/${GODOT_VERSION%-*}.stable/ \
    && rm -rf /tmp/templates /tmp/templates.tpz

WORKDIR /src
COPY . /src

# Roda o build script — gera out/web/index.html e out/headless/yggdrasil-godot.
RUN bash /src/scripts/build.sh all

# --- Stage 2: runtime ---------------------------------------------------------
FROM debian:trixie-slim AS runtime

ENV DEBIAN_FRONTEND=noninteractive

# Dependências mínimas para rodar binário Godot headless.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libfontconfig1 \
        libfreetype6 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash godot

WORKDIR /app

# Copia somente os artefatos exportados.
COPY --from=builder /src/out/headless /app/headless
COPY --from=builder /src/out/web /app/web

RUN chown -R godot:godot /app

USER godot

# Porta dedicada do POC Godot, separada da :3030 do yggdrasil-web Rust.
EXPOSE 3031

ENV YGGDRASIL_GODOT_PORT=3031

# Roda o binário headless como server. Argumentos finais ficam à mercê
# do que o POC precisar (multiplayer host, etc.) — por ora, apenas roda
# a scene principal em modo headless com renderer dummy.
CMD ["/app/headless/yggdrasil-godot", "--headless", "--rendering-driver", "dummy"]
