#!/usr/bin/env bash
set -Eeuo pipefail

declare -A aliases=(
	[1.19.0]='1 1.19 latest'
)

defaultDebianSuite='stretch'
declare -A debianSuite=(
)

self="$(basename "$BASH_SOURCE")"
cd "$(dirname "$(readlink -f "$BASH_SOURCE")")"

source '.metadata-lib'

versions=( */ )
versions=( "${versions[@]%/}" )

# sort version numbers with highest first
IFS=$'\n'; versions=( $(echo "${versions[*]}" | sort -rV) ); unset IFS

# get the most recent commit which modified any of "$@"
fileCommit() {
	git log -1 --format='format:%H' HEAD -- "$@"
}

# get the most recent commit which modified "$1/Dockerfile" or any file COPY'd from "$1/Dockerfile"
dirCommit() {
	local dir="$1"; shift
	(
		cd "$dir"
		fileCommit \
			Dockerfile \
			$(git show HEAD:./Dockerfile | awk '
				toupper($1) == "COPY" {
					for (i = 2; i < NF; i++) {
						print $i
					}
				}
			')
	)
}

cat <<-EOH
# this file is generated via https://github.com/sfackler/docker-rust/blob/$(fileCommit "$self")/$self

Maintainers: Steven Fackler <sfackler@gmail.com> (@sfackler)
GitRepo: https://github.com/sfackler/docker-rust.git
EOH

# prints "$2$1$3$1...$N"
join() {
	local sep="$1"; shift
	local out; printf -v out "${sep//%/%%}%s" "$@"
	echo "${out#$sep}"
}

for version in "${versions[@]}"; do
	versionAliases=(
		$version
	)
	versionAliases+=(
		${aliases[$version]:-}
	)

	for v in \
			stretch jessie \
	; do
		dir="$version/$v"

		[ -f "$dir/Dockerfile" ] || continue

		variant="$(basename "$v")"
		versionSuite="${debianSuite[$version]:-$defaultDebianSuite}"

		commit="$(dirCommit "$dir")"

		baseAliases=( "${versionAliases[@]}" )
		variantAliases=( "${baseAliases[@]/%/-$variant}" )
		variantAliases=( "${variantAliases[@]//latest-/}" )

		if [ "$variant" = "$versionSuite" ]; then
			variantAliases+=( "${baseAliases[@]}" )
		fi

		case "$v" in
			*)  variantArches="$(variantArches "$version" "$v")" ;;
		esac

		echo
		cat <<-EOE
			Tags: $(join ', ' "${variantAliases[@]}")
			Architectures: $(join ', ' $variantArches)
			GitCommit: $commit
			Directory: $dir
		EOE
        [ "$variant" = "$v" ] || echo "Constraints: $variant"
	done
done
