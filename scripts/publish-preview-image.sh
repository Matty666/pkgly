#!/usr/bin/env bash
# ABOUTME: Publishes the tested OIDC preview image in the selected fork.
# ABOUTME: Checks the source branch and image revision before publication.
set -euo pipefail

if [[ ${GITHUB_REPOSITORY:-} != Matty666/pkgly ||
      ${GITHUB_HEAD_REPOSITORY:-} != "$GITHUB_REPOSITORY" ||
      ${GITHUB_HEAD_REF:-} != feature/generic-oidc ]]; then
    echo 'Preview publication requires the selected fork and OIDC branch.' >&2
    exit 1
fi

if [[ ! ${PKGLY_IMAGE_REVISION:-} =~ ^[0-9a-f]{40}$ ]]; then
    echo 'The image revision must be a full lowercase commit SHA.' >&2
    exit 1
fi

image_revision="$(docker image inspect pkgly:test --format '{{ index .Config.Labels "org.opencontainers.image.revision" }}')"
if [[ $image_revision != "$PKGLY_IMAGE_REVISION" ]]; then
    echo 'The tested image revision does not match the selected commit.' >&2
    exit 1
fi

image="ghcr.io/${GITHUB_REPOSITORY,,}"
docker tag pkgly:test "${image}:${PKGLY_IMAGE_REVISION}"
docker push "${image}:${PKGLY_IMAGE_REVISION}"
docker tag pkgly:test "${image}:generic-oidc"
docker push "${image}:generic-oidc"

if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    {
        printf 'Published image: `%s:%s`.\n\n' "$image" "$PKGLY_IMAGE_REVISION"
        printf 'Platform: `linux/amd64`.\n\n'
        printf '```sh\ndocker pull %s:%s\n```\n' "$image" "$PKGLY_IMAGE_REVISION"
    } >> "$GITHUB_STEP_SUMMARY"
fi
