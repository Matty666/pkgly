## Publishing

Use `npm publish --tag beta` to publish a prerelease without changing `latest`:

```sh
npm publish --tag beta
npm install my-package@beta
npm install my-package@1.2.0-beta.1
```

Pkgly stores the `dist-tags` from each publish request. A publish changes only the supplied tags.
The default npm tag is `latest`, including when a client explicitly publishes a prerelease with that tag.
Pkgly does not infer a tag from a prerelease suffix.

Use npm to manage tags after publication:

```sh
npm dist-tag ls my-package
npm dist-tag add my-package@1.2.0-beta.1 beta
npm dist-tag add my-package@1.2.0 latest
npm dist-tag rm my-package beta
```

Scoped packages use the same commands with names such as `@scope/my-package`.
Exact version downloads preserve prerelease identifiers and build metadata.

Existing packages without stored tags use their most recent stable upload for `latest`.
Packages with only prereleases have no inferred `latest` tag.
Old publish tags cannot be recovered because earlier Pkgly versions discarded them.
Use `npm dist-tag add` to restore those tags.

### Database upgrades

The upgrade adds `projects.npm_dist_tags` as a nullable JSON column.
It preserves existing package versions and metadata. Startup applies upgrade migrations only.
Rollback files run only when an operator requests a rollback.
An explicit rollback removes the tag column and loses tags written after the upgrade.
Package versions and metadata remain intact.

## Logging in

When you run `npm adduser` it will first send a request to `{registry_url}/-/v1/login`
if this path resolves it should return a web login response. However, this feature is not documented here or implemented in Pkgly so this returns a bad request.

So if that requests returns a bad request. It will attempt to do a couch db login. 


### Couch DB Login

Which is at route PUT `-/user/org.couchdb.user:{SOME_USER}`

SOME_USER being the username.

The body will be `application/json` with
```json
{
    "name": "Username",
    "password": "Password",
    "email": "Email",
    "login_type": "user",
    "roles": [],
    "date": "CURRENT_ISO_8601_TIMESTAMP"
}
```

You will now validate the login and return a token if successful.

Response is `application/json` with
```json
{
    "token": "TOKEN"
}
```
NPM will save this to the config file and you will be logged in.