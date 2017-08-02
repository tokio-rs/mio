FROM buildpack-deps:stretch

ENV RUST_VERSION=1.19.0
ENV RUSTUP_VERSION=1.5.0

RUN set -eux; \
    \
# this "case" statement is generated via "update.sh"
    dpkgArch="$(dpkg --print-architecture)"; \
	case "${dpkgArch##*-}" in \
		amd64) rustArch='x86_64-unknown-linux-gnu'; rustupSha256='7b5ce33a881992b285e2aa6cbc785da4138c5bab7c8c9b55c06918bfb1ba0efa' ;; \
		armhf) rustArch='armv7-unknown-linux-gnueabihf'; rustupSha256='a92b003a15b2e4bd240c0f1d46232958c173f5605814e19961fc8a4d99a25b3e' ;; \
		i386) rustArch='i686-unknown-linux-gnu'; rustupSha256='4a478c977b7b4900456c2d4dd165019e7c923ebdaba3f47316717d1690387d9a' ;; \
		ppc64el) rustArch='powerpc64le-unknown-linux-gnu'; rustupSha256='5a966b6dd113995ff2e9cb59e9fd36aa2ce378bb1a397f27ceaeb41df5cf34c8' ;; \
		s390x) rustArch='s390x-unknown-linux-gnu'; rustupSha256='e89be1ac02188cbe181c00725a036f814817be337a6480cdcc83b3fa98acb875' ;; \
		*) echo >&2 "unsupported architecture: ${dpkgArch}"; exit 1 ;; \
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
