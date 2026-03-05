ARG VARIANT="ubuntu-24.04"
FROM mcr.microsoft.com/devcontainers/base:${VARIANT}

ARG TARGETARCH

# =========================================================
# Pinned versions — change here to upgrade
# =========================================================
ARG GO_VERSION="1.22.4"
ARG ECHIDNA_VERSION="2.3.1"
ARG ADERYN_VERSION="0.6.8"
ARG MEDUSA_VERSION="v1.5.0"

# =========================================================
# System dependencies
# =========================================================
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    git \
    libgmp-dev \
    python3 \
    python3-pip \
    python3-venv \
    python3-dev \
    && rm -rf /var/lib/apt/lists/*

# =========================================================
# Go (official binary, arch-aware — needed for Medusa)
# =========================================================
RUN curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${TARGETARCH}.tar.gz" \
    | tar -C /usr/local -xz \
    && ln -s /usr/local/go/bin/go /usr/local/bin/go \
    && ln -s /usr/local/go/bin/gofmt /usr/local/bin/gofmt

# =========================================================
# Python audit venv (shared, root-owned, world-readable)
# =========================================================
RUN python3 -m venv /opt/audit-venv \
    && /opt/audit-venv/bin/pip install --no-cache-dir --upgrade pip

# =========================================================
# Slither + Halmos
# =========================================================
RUN /opt/audit-venv/bin/pip install --no-cache-dir \
    slither-analyzer \
    halmos

# =========================================================
# Echidna (x86_64 static binary)
# NOTE: No native ARM64 Linux binary exists. On Apple Silicon,
# Docker Desktop provides QEMU-based x86_64 emulation.
# Fuzzing performance will be reduced on ARM64.
# =========================================================
RUN curl -fsSL \
    "https://github.com/crytic/echidna/releases/download/v${ECHIDNA_VERSION}/echidna-${ECHIDNA_VERSION}-x86_64-linux.tar.gz" \
    -o /tmp/echidna.tar.gz \
    && tar -xzf /tmp/echidna.tar.gz -C /usr/local/bin/ \
    && chmod +x /usr/local/bin/echidna \
    && rm /tmp/echidna.tar.gz

# =========================================================
# Medusa (build from source via go install — native on any arch)
# =========================================================
ENV GOPATH="/tmp/go"
RUN go install "github.com/crytic/medusa@${MEDUSA_VERSION}" \
    && mv /tmp/go/bin/medusa /usr/local/bin/medusa \
    && rm -rf /tmp/go

# =========================================================
# Aderyn (arch-aware binary — publishes x86_64 and aarch64)
# =========================================================
RUN set -eux; \
    case "${TARGETARCH}" in \
        amd64) ADERYN_ARCH="x86_64" ;; \
        arm64) ADERYN_ARCH="aarch64" ;; \
        *) echo "Unsupported arch: ${TARGETARCH}" && exit 1 ;; \
    esac; \
    curl -fsSL \
        "https://github.com/Cyfrin/aderyn/releases/download/aderyn-v${ADERYN_VERSION}/aderyn-${ADERYN_ARCH}-unknown-linux-gnu.tar.xz" \
        -o /tmp/aderyn.tar.xz \
    && tar -xJf /tmp/aderyn.tar.xz -C /tmp/ \
    && mv /tmp/aderyn-${ADERYN_ARCH}-unknown-linux-gnu/aderyn /usr/local/bin/aderyn \
    && chmod +x /usr/local/bin/aderyn \
    && rm -rf /tmp/aderyn.tar.xz /tmp/aderyn-${ADERYN_ARCH}-unknown-linux-gnu

# =========================================================
# Global PATH for all users (system tools + Python venv)
# =========================================================
RUN echo 'export PATH="/opt/audit-venv/bin:/usr/local/bin:$PATH"' \
    > /etc/profile.d/audit-tools.sh \
    && chmod +x /etc/profile.d/audit-tools.sh

# =========================================================
# Foundry (must install as vscode user — installs to ~/.foundry/bin)
# =========================================================
USER vscode
SHELL ["/bin/bash", "-c"]

ENV PATH="/home/vscode/.foundry/bin:${PATH}"

RUN curl -fsSL https://foundry.paradigm.xyz | bash \
    && foundryup

# =========================================================
# Ensure vscode user has venv on PATH in interactive shells
# =========================================================
RUN echo 'export PATH="/opt/audit-venv/bin:$PATH"' >> /home/vscode/.bashrc

USER vscode
