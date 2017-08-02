FROM buildpack-deps:jessie

ENV RUST_VERSION=1.19.0
ENV RUSTUP_VERSION=1.5.0

RUN set -eux; \
    \
    dpkgArch="$(dpkg --print-architecture)"; \
    case "${dpkgArch##*-}" in \
        amd64) rustArch='x86_64-unknown-linux-gnu'; rustupSha256='7b5ce33a881992b285e2aa6cbc785da4138c5bab7c8c9b55c06918bfb1ba0efa' ;; \
        armhf) rustArch='arm-unknown-linux-gnueabihf'; rustupSha256='d256b0a32115a50aeb184e73a222835716e5ea9d876a954fc94e56065ae81f99' ;; \
        i386) rustArch='i686-unknown-linux-gnu'; rustupSha256='4a478c977b7b4900456c2d4dd165019e7c923ebdaba3f47316717d1690387d9a' ;; \
        *) echo >&2 "unsupported architecture ($dpkgArch)"; exit 1 ;; \
    esac; \
    \
    url="https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${rustArch}/rustup-init"; \
    wget "$url"; \
    echo "${rustupSha256} *rustup-init" | sha256sum -c -; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --default-toolchain "${RUST_VERSION}"; \
    rm rustup-init; \
    export PATH="/root/.cargo/bin:$PATH"; \
    rustup --version; \
    cargo --version; \
    rustc --version;

ENV PATH /root/.cargo/bin:$PATH
