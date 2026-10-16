# NPM Registry HTTP Routes

Pkgly implements a subset of the npm registry API.
All paths below are relative to `/repositories/<storage>/<repository>/`.
There is no extra `package/` prefix before a package name.
For scoped package metadata, npm encodes the slash, for example `@scope%2Fmy-package`.
Tarball paths can contain the unencoded scope and package name.

## Package reads

Hosted repositories support these routes:

| Method | Path | Result |
| --- | --- | --- |
| GET | `<name>` | Package metadata, versions, README, and distribution tags |
| GET | `<name>/<version>` | Metadata for one exact version |
| GET | `<name>/<tag>` | Metadata for the version selected by a tag |
| GET | `<name>/-/<basename>-<version>.tgz` | Package archive |

For `@scope/my-package`, the basename is `my-package`.
Prerelease identifiers and build metadata remain part of the exact version.
Use authentication headers when the repository requires authentication.

```sh
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/@scope%2Fmy-package"
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/@scope%2Fmy-package/beta"
curl -H "Authorization: Bearer <token>" \
     -O "https://<host>/repositories/<storage>/<repository>/@scope/my-package/-/my-package-1.2.0-beta.1.tgz"
```

Proxy repositories cache upstream metadata and tarballs.
Virtual repositories resolve reads through their configured members and forward publication to their configured hosted target.

## Publish a new version

Use `npm publish` with the repository URL as the registry:

```sh
npm publish --registry="https://<host>/repositories/<storage>/<repository>/" --tag beta
```

The client sends `PUT <name>` with `Npm-Command: publish`.
Its JSON body contains the package `name`, one entry in `versions`, and base64 tarball data in `_attachments`.
Each version includes its manifest, `dist` hashes and tarball URL, `_id`, `_nodeVersion`, and `_npmVersion`.
The tarball URL must point into the target storage and repository.
Metadata and archive upload form one publish request. Pkgly does not support a separate archive upload endpoint.

The `dist-tags` object controls which tags change.
Missing or empty tags default to `latest`, including for prereleases.
An explicit `beta` tag leaves `latest` unchanged. An explicit `latest` can select a prerelease.
See [Publishing](standard.md#publishing) for client behavior and examples.

## Update package metadata

For README or ordinary manifest changes, edit the package files and publish a new version.
The README normally comes from `README.md` beside `package.json`.
Pkgly stores README content supplied in the version metadata.
If that content is absent, it accepts `readme` and `readmeFilename` from the root of the publish body.

The package response exposes the README and public metadata from the version selected by `latest`.
If there is no `latest` tag, it uses the most recent published version.
Public fields include description, author, contributors, license, keywords, homepage, bugs, repository, and maintainers.
Older versions keep their own metadata.

```sh
npm version patch
npm publish --registry="https://<host>/repositories/<storage>/<repository>/"
npm view my-package readme --registry="https://<host>/repositories/<storage>/<repository>/"
```

Pkgly does not offer arbitrary edits to an existing version's manifest or a standalone package README update.
The previous `/package/<name>?rev=...` example was not a supported Pkgly route or an npm CLI README workflow.
The separate operations below change tags and deprecation messages without a new publication.

## Manage distribution tags

| Method | Path | Request body |
| --- | --- | --- |
| GET | `-/package/<name>/dist-tags` | None |
| PUT | `-/package/<name>/dist-tags/<tag>` | A JSON string with an existing version |
| DELETE | `-/package/<name>/dist-tags/<tag>` | None |

For scoped packages, `<name>` includes the scope, such as `@scope/my-package`.
Tag changes require repository write permission.

```sh
npm dist-tag ls my-package
npm dist-tag add my-package@1.2.0-beta.1 beta
npm dist-tag rm my-package beta
```

The equivalent HTTP request to set a tag is:

```sh
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '"1.2.0-beta.1"' \
     "https://<host>/repositories/<storage>/<repository>/-/package/my-package/dist-tags/beta"
```

## Deprecate and undeprecate versions

Use the npm CLI to select a version or version range:

```sh
npm deprecate 'my-package@1.x' 'Use version 2 instead'
npm deprecate 'my-package@1.x' ''
```

The CLI first reads the package document with `GET <name>?write=true`.
It sets `deprecated` on the matching versions, then sends the document to `PUT <name>` with `Npm-Command: deprecate`.
Pkgly also accepts `Npm-Command: undeprecate` for clients with a separate undeprecate command.
An empty message clears the warning.

Pkgly changes only the supplied versions' `deprecated` fields.
It preserves their other metadata, tarballs, and distribution tags.
The entire update fails if a target version is missing or a deprecation message is not a string.
These changes require repository write permission.

## Repository management API

These Pkgly management endpoints are separate from the npm registry protocol:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/repository/<repository-id>/packages?page=1&per_page=50` | List stored packages |
| DELETE | `/api/repository/<repository-id>/packages` | Delete paths returned by the package list |
| GET | `/api/repository/<repository-id>/config/npm` | Read npm repository configuration |
| PUT | `/api/repository/<repository-id>/config/npm` | Change npm repository configuration |

Deletion requires repository edit permission and a JSON body such as `{"paths":["<path-from-package-list>"]}`.
It is not an implementation of `npm unpublish`.

```sh
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/npm"
```

A proxy configuration uses the following shape:

```json
{
  "type": "Proxy",
  "config": {
    "routes": [
      {"url": "https://registry.npmjs.org", "name": "NPM Official"}
    ]
  }
}
```

## Authentication and unsupported operations

Use a Pkgly repository token in `.npmrc`, or log in with an existing Pkgly account:

```sh
npm login --auth-type=legacy --registry="https://<host>/repositories/<storage>/<repository>/"
```

Legacy login uses `PUT -/user/org.couchdb.user:<username>` and returns a repository token.
It does not register a new Pkgly account. Web login is not implemented.

Hosted repositories do not currently implement npm unpublish, registry search, registry statistics, `whoami`, or CouchDB session endpoints.
A root registry information document and an attachments listing endpoint are also not implemented.
These limits do not prevent package publication, installation, tag management, or deprecation through npm.
