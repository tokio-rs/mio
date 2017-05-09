FROM ubuntu:16.04

RUN dpkg --add-architecture amd64 && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
  file \
  curl \
  ca-certificates \
  python \
  unzip \
  expect \
  openjdk-9-jre \
  libstdc++6:amd64 \
  gcc \
  libc6-dev \
  libpulse0:amd64

COPY cargo_config /etc/cargo_config

WORKDIR /android/

COPY install-ndk.sh /android/
RUN sh /android/install-ndk.sh

ENV PATH=$PATH:/android/ndk-arm64/bin:/android/sdk/tools:/android/sdk/platform-tools

COPY install-sdk.sh accept-licenses.sh /android/
RUN sh /android/install-sdk.sh

ENV PATH=$PATH:/rust/bin \
    CARGO_TARGET_ARM_LINUX_ANDROIDEABI_LINKER=aarch64-linux-android-gcc \
    ANDROID_EMULATOR_FORCE_32BIT=0 \
    HOME=/tmp
RUN chmod 755 /android/sdk/tools/* /android/sdk/tools/qemu/linux-x86_64/* /android/sdk/tools/qemu/linux-x86/*

RUN cp -r /root/.android /tmp
RUN chmod 777 -R /tmp/.android
