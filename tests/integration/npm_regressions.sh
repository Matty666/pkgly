#!/bin/bash
# Sourced by test_npm.sh after hosted packages and the virtual registry exist.

# shellcheck source=tests/integration/common.sh

cd "$WORKSPACE/hello-pkg"
REG_TAG_PATH="/repositories/${NPM_HOSTED_REPO}/-/package/${ENCODED_NAME}/dist-tags"

for REG_BODY in '{' '{}' '[]' 'null' '123' '"9.9.9"'; do
    print_test "Tag update rejects invalid body ${REG_BODY} without changing tags"
    REG_BEFORE=$(api_get "$REG_TAG_PATH")
    REG_EXPECTED=400
    if [ "$REG_BODY" = '"9.9.9"' ]; then REG_EXPECTED=404; fi
    REG_STATUS=$(curl -s -o "$WORKSPACE/tag-error.txt" -w '%{http_code}' -X PUT \
        -H "$(get_auth_header)" -H 'Content-Type: application/json' --data "$REG_BODY" \
        "${PKGLY_URL}${REG_TAG_PATH}/invalid-body")
    REG_AFTER=$(api_get "$REG_TAG_PATH")
    if [ "$REG_STATUS" = "$REG_EXPECTED" ] && \
       jq -e --argjson before "$REG_BEFORE" '. == $before' <<<"$REG_AFTER" > /dev/null; then
        pass
    else
        fail "Expected ${REG_EXPECTED} with unchanged tags, got ${REG_STATUS}"
    fi
done

for REG_NAME in "$PACKAGE_NAME" "$UNSCOPED_NAME"; do
    REG_VERSION="$VERSION_1"
    for REG_TAG in 'release!' 'release(candidate)' "release'candidate"; do
        print_test "Native npm accepts tag ${REG_TAG} for ${REG_NAME}"
        if run_cmd npm dist-tag add "${REG_NAME}@${REG_VERSION}" "$REG_TAG" && \
           [ "$(npm view "${REG_NAME}@${REG_TAG}" version)" = "$REG_VERSION" ] && \
           run_cmd npm dist-tag rm "$REG_NAME" "$REG_TAG"; then
            pass
        else
            fail "Could not add, resolve, and remove the npm-compatible tag"
        fi
    done
done

print_test "Tag names Beta and beta remain distinct through HTTP and npm"
if run_cmd npm dist-tag add "${PACKAGE_NAME}@${VERSION_1}" Beta && \
   run_cmd npm dist-tag add "${PACKAGE_NAME}@${VERSION_2}" beta && \
   [ "$(npm view "${PACKAGE_NAME}@Beta" version)" = "$VERSION_1" ] && \
   [ "$(npm view "${PACKAGE_NAME}@beta" version)" = "$VERSION_2" ]; then
    pass
else
    fail "Tag casing was lost"
fi

for REG_SLASH in '%2f' '%2F'; do
    print_test "Encoded scoped tag and metadata paths accept ${REG_SLASH}"
    REG_ENCODED="%40pkgly-test${REG_SLASH}hello-pkg"
    REG_METADATA=$(api_get "/repositories/${NPM_HOSTED_REPO}/${REG_ENCODED}" || echo '{}')
    REG_TAGS=$(api_get "/repositories/${NPM_HOSTED_REPO}/-/package/${REG_ENCODED}/dist-tags" || echo '{}')
    if jq -e --arg name "$PACKAGE_NAME" '.name == $name' <<<"$REG_METADATA" > /dev/null && \
       jq -e --arg version "$VERSION_1" '.Beta == $version' <<<"$REG_TAGS" > /dev/null; then
        pass
    else
        fail "Encoded slash changed scoped package routing"
    fi
done

for REG_PATH in '@pkgly-test' \
    '@pkgly-test/hello-pkg/not-a-marker/hello-pkg-1.0.0.tgz' \
    '@pkgly-test/hello-pkg/-/@different/hello-pkg-1.0.0.tgz' \
    'pkgly-prerelease-test/not-a-marker/pkgly-prerelease-test-1.0.0.tgz'; do
    print_test "Malformed npm path returns a client error: ${REG_PATH}"
    REG_STATUS=$(curl -s -o /dev/null -w '%{http_code}' -H "$(get_auth_header)" \
        "${PKGLY_URL}/repositories/${NPM_HOSTED_REPO}/${REG_PATH}" || true)
    if [ "$REG_STATUS" = 400 ] || [ "$REG_STATUS" = 404 ]; then
        pass
    else
        fail "Malformed path returned ${REG_STATUS} instead of 400/404"
    fi
done

REG_CASE_DIR="$WORKSPACE/case-package"
mkdir -p "$REG_CASE_DIR"
cp "$FIXTURE_DIR/"* "$REG_CASE_DIR/"
cp "$WORKSPACE/hello-pkg/.npmrc" "$REG_CASE_DIR/.npmrc"
cd "$REG_CASE_DIR"
jq '.name = "pkgly-case-test" | .version = "1.0.0-RC.1"' package.json > package.json.tmp
mv package.json.tmp package.json
print_test "Uppercase prerelease identifiers survive publication and fresh installation"
if run_cmd npm publish --tag next; then
    mkdir -p "$WORKSPACE/install-case"
    cp .npmrc "$WORKSPACE/install-case/.npmrc"
    cd "$WORKSPACE/install-case"
    if run_cmd npm install pkgly-case-test@next --cache "$WORKSPACE/cache-case" && \
       [ "$(jq -r .version node_modules/pkgly-case-test/package.json)" = '1.0.0-RC.1' ]; then
        pass
    else
        fail "Uppercase prerelease did not install"
    fi
else
    fail "Uppercase prerelease did not publish"
fi

cd "$REG_CASE_DIR"
jq '.version = "1.0.0-RC.2"' package.json > package.json.tmp
mv package.json.tmp package.json
printf '//%s/repositories/TEST-STORAGE/NPM-HOSTED/:_authToken=%s\n' \
    "$PKGLY_REGISTRY_HOST" "$TEST_TOKEN" >> .npmrc
print_test "Publication matches case-insensitive storage and repository lookup"
if run_cmd npm publish --tag next --registry="${PKGLY_URL}/repositories/TEST-STORAGE/NPM-HOSTED/" && \
   [ "$(npm view pkgly-case-test@next version)" = '1.0.0-RC.2' ]; then
    pass
else
    fail "Mixed-case registry URL did not publish through the same repository"
fi

cd "$VIRTUAL_INSTALL_DIR"
for REG_NAME in "$PACKAGE_NAME" "$UNSCOPED_NAME"; do
    print_test "Virtual registry lists tags for ${REG_NAME}"
    if run_cmd npm dist-tag ls "$REG_NAME"; then pass; else fail "Virtual tag list failed"; fi
    print_test "Virtual registry adds a tag for ${REG_NAME}"
    if run_cmd npm dist-tag add "${REG_NAME}@${VERSION_1}" virtual-review; then
        REG_TAGS=$(api_get "/repositories/${NPM_HOSTED_REPO}/-/package/${REG_NAME}/dist-tags")
        if jq -e --arg version "$VERSION_1" '.["virtual-review"] == $version' <<<"$REG_TAGS" > /dev/null; then
            pass
        else
            fail "Virtual tag addition did not reach the hosted target"
        fi
    else
        fail "Virtual tag addition failed"
    fi
    print_test "Virtual registry removes a hosted tag for ${REG_NAME}"
    # Seed directly so DELETE is exercised even if the preceding virtual addition failed.
    api_put "/repositories/${NPM_HOSTED_REPO}/-/package/${REG_NAME}/dist-tags/virtual-review" \
        -H 'Content-Type: application/json' --data '"1.0.0"' > /dev/null
    REG_STATUS=$(curl -s -o /dev/null -w '%{http_code}' -X DELETE -H "$(get_auth_header)" \
        "${PKGLY_URL}/repositories/${NPM_VIRTUAL_REPO}/-/package/${REG_NAME}/dist-tags/virtual-review")
    REG_TAGS=$(api_get "/repositories/${NPM_HOSTED_REPO}/-/package/${REG_NAME}/dist-tags")
    if [[ "$REG_STATUS" = 2* ]] && jq -e 'has("virtual-review") | not' <<<"$REG_TAGS" > /dev/null; then
        pass
    else
        fail "Virtual tag removal returned ${REG_STATUS} or left the tag on the hosted target"
    fi
done

REG_READ_TOKEN='npm-read-only-regression-token'
print_test "Read-only token can read hosted tags"
REG_STATUS=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer ${REG_READ_TOKEN}" \
    "${PKGLY_URL}${REG_TAG_PATH}")
if [ "$REG_STATUS" = 200 ]; then pass; else fail "Read-only token could not read tags"; fi

for REG_METHOD in PUT DELETE; do
    print_test "Read-only token cannot ${REG_METHOD} hosted tags"
    REG_BEFORE=$(api_get "$REG_TAG_PATH")
    REG_STATUS=$(curl -s -o /dev/null -w '%{http_code}' -X "$REG_METHOD" \
        -H "Authorization: Bearer ${REG_READ_TOKEN}" -H 'Content-Type: application/json' \
        --data '"1.0.0"' "${PKGLY_URL}${REG_TAG_PATH}/Beta")
    REG_AFTER=$(api_get "$REG_TAG_PATH")
    if { [ "$REG_STATUS" = 401 ] || [ "$REG_STATUS" = 403 ]; } && \
       jq -e --argjson before "$REG_BEFORE" '. == $before' <<<"$REG_AFTER" > /dev/null; then
        pass
    else
        fail "Read-only token changed tags or returned ${REG_STATUS}"
    fi
done

print_test "Read-only token cannot deprecate a hosted version"
REG_BEFORE=$(api_get "$METADATA_PATH")
jq '.versions["1.0.0"].deprecated = "must not save"' <<<"$REG_BEFORE" > "$WORKSPACE/read-only-update.json"
REG_STATUS=$(curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer ${REG_READ_TOKEN}" \
    -H 'Content-Type: application/json' -H 'npm-command: deprecate' \
    --data-binary "@$WORKSPACE/read-only-update.json" "${PKGLY_URL}${METADATA_PATH}")
REG_AFTER=$(api_get "$METADATA_PATH")
if { [ "$REG_STATUS" = 401 ] || [ "$REG_STATUS" = 403 ]; } && \
   jq -e --argjson before "$REG_BEFORE" '. == $before' <<<"$REG_AFTER" > /dev/null; then
    pass
else
    fail "Read-only token changed metadata or returned ${REG_STATUS}"
fi
