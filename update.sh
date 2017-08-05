#!/usr/bin/env bash
set -Eeuo pipefail

cd "$(dirname "$(readlink -f "$BASH_SOURCE")")"

source '.metadata-lib'

versions=( "$@" )
if [ ${#versions[@]} -eq 0 ]; then
    versions=( */ )
fi
versions=( "${versions[@]%/}" )

# see http://stackoverflow.com/a/2705678/433558
sed_escape_rhs() {
	echo "$@" | sed -e 's/[\/&]/\\&/g' | sed -e ':a;N;$!ba;s/\n/\\n/g'
}

travisEnv=
for version in "${versions[@]}"; do
    rustupVersion=$(rustupVersion "$version")
    linuxArchCase='dpkgArch="$(dpkg --print-architecture)"; '$'\\\n'
    linuxArchCase+=$'\t''case "${dpkgArch##*-}" in '$'\\\n'
    for dpkgArch in $(dpkgArches "$version"); do
        rustArch="$(dpkgToRustArch "$version" "$dpkgArch")"
        sha256="$(curl -fsSL "https://static.rust-lang.org/rustup/archive/${rustupVersion}/${rustArch}/rustup-init.sha256" | awk '{ print $1 }')"
        linuxArchCase+=$'\t\t'"$dpkgArch) rustArch='$rustArch'; rustupSha256='$sha256' ;; "$'\\\n'
    done
    linuxArchCase+=$'\t\t''*) echo >&2 "unsupported architecture: ${dpkgArch}"; exit 1 ;; '$'\\\n'
    linuxArchCase+=$'\t''esac'

    for variant in jessie stretch; do
        if [ -d "$version/$variant" ]; then
            sed -r \
                -e 's!%%RUST-VERSION%%!'"$version"'!g' \
                -e 's!%%RUSTUP-VERSION%%!'"$rustupVersion"'!g' \
                -e 's!%%DEBIAN-SUITE%%!'"$variant"'!g' \
                -e 's!%%ARCH-CASE%%!'"$(sed_escape_rhs "$linuxArchCase")"'!g' \
                Dockerfile-debian.template > "$version/$variant/Dockerfile"
                travisEnv='\n  - VERSION='"$version VARIANT=$variant$travisEnv"
        fi
    done
done

travis="$(awk -v 'RS=\n\n' '$1 == "env:" { $0 = "env:'"$travisEnv"'" } { printf "%s%s", $0, RS }' .travis.yml)"
echo "$travis" > .travis.yml
