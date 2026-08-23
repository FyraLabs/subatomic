# Subatomic (v1) API Documentation

## Why Not OpenAPI?

`utoipa` does not support Rust nightly.

## Authentication

Use `Authentication: Bearer <jwt>` header.

## Endpoints

| Method   | Endpoint                   | Rust Function         |
|----------|----------------------------|-----------------------|
| `GET`    | `/v1/repos`                | `repos::list_repos`   |
| `PUT`    | `/v1/repos/{name}`         | `repos::create_repo`  |
| `DELETE` | `/v1/repos/{name}`         | `repos::delete_repo`  |
| `POST`   | `/v1/repos/{name}`         | `repos::upload_pkgs`  |
| `PUT`    | `/v1/repos/{name}/comps`   | `repos::push_comps`   |
| `DELETE` | `/v1/repos/{name}/comps`   | `repos::del_comps`    |
| `GET`    | `/v1/repos/{name}/key`     | `repos::get_key`      |
| `PUT`    | `/v1/repos/{name}/key`     | `repos::set_key`      |
| `DELETE` | `/v1/repos/{name}/key`     | `repos::del_key`      |
| `GET`    | `/v1/repos/{name}/rpms`    | `repos::list_rpms`    |
| `POST`   | `/v1/repos/{name}/rpms`    | `repos::del_rpms`     |
| `POST`   | `/v1/repos/{name}/refresh` | `repos::refresh_repo` |
| `POST`   | `/v1/repos/{name}/rebuild` | `repos::rebuild_repo` |
| `PUT`    | `/v1/repos/{name}/md/{md}` | `repos::upl_md`       |
| `DELETE` | `/v1/repos/{name}/md/{md}` | `repos::del_md`       |
| `POST`   | `/v1/keys`                 | `keys::create_key`    |
| `GET`    | `/v1/keys`                 | `keys::list_keys`     |
| `GET`    | `/v1/keys/{id}`            | `keys::get_key`       |
| `DELETE` | `/v1/keys/{id}`            | `keys::del_key`       |

### `GET /v1/repos`: List repositories

- response: `Repo`

### `PUT /v1/repos/{name}`: Create repository

- response: `Repo`

### `DELETE /v1/repos/{name}`: Delete repository

### `POST /v1/repos/{name}`: Upload packages

- request: a multipart consisting of fields of files, where the field name should be the filename.
- response:
  ```json
  {
    "added": [
      { "pkg": "<byte filename>", "sig": "[byte signature]" },
      ...
    ],
    "bad_filenames": ["<byte filename>", ...],
    "removed": ["<byte filename>", ...]
  }
  ```

### `PUT /v1/repos/{name}/comps`: [DEPRECATED] Push comps

- request: multipart (1 field for comps file)
- response: 204 NO CONTENT | 404 NOT FOUND
- DEPRECATED: use `PUT /v1/repos/{name}/md/{md}`

### `DELETE /v1/repos/{name}/comps`: [DEPRECATED] Delete comps

- response: 204 NO CONTENT | 404 NOT FOUND
  - note that if comps originally did not exist, 204 is returned.
  - 404 indicates that repo `{name}` is not found.
- DEPRECATED: use `DELETE /v1/repos/{name}/md/{md}`

### `GET /v1/repos/{name}/key`: Get signing key

- response: public armor in raw text

### `PUT /v1/repos/{name}/key`: Set signing key

- request: `{ id: <i32_signing_key_id> }`
  - obtain the id from `GET /v1/keys`
- response: 204 NO CONTENT | 404 NOT FOUND

### `DELETE /v1/repos/{name}/key`: Remove signing key

- response: 204 NO CONTENT | 404 NOT FOUND

### `GET /v1/repos/{name}/rpms`: List RPMs

- response:
  ```json
  ["<byte filename>", ...]
  ```

### `POST /v1/repos/{name}/rpms`: **DELETE** RPMs

> [!WARNING]
> This endpoint is for **batch-deleting** RPMs.
> Note that at of the time of writing this documentation, this is the only endpoint for deleting RPMs.

- request:
  ```json
  { "rpms": ["<filename>", ...] }
  ```
- response: 200 OK
  ```json
  { "not_found": ["<filename>", ...] }
  ```

### `POST /v1/repos/{name}/refresh`: Refresh repository

This regenerates the repository metadata (most things in `repodata/`) with the use of the cache.

- response: 204 NO CONTENT | 404 NOT FOUND

### `POST /v1/repos/{name}/rebuild`: Rebuild repository

This invalidates the cache then regenerates the repository metadata.

- response: 204 NO CONTENT | 404 NOT FOUND

### `PUT /v1/repos/{name}/md/{md}`: Upload metadata

- `{md}`: as in `<data type="{md}" />`, the type to be used in `repomd.xml`
- request: a single-field multipart.
- behaviour: the file is compressed and written to `repodata/{sha}-{filename}.zst`,
  where `{filename}` is taken from `field.file_name()`.
- response: 204 NO CONTENT | 404 NOT FOUND

### `DELETE /v1/repos/{name}/md/{md}`: Delete metadata

- `{md}`: as in `<data type="{md}" />`, the type to be used in `repomd.xml`
- response: 204 NO CONTENT | 404 NOT FOUND

### `POST /v1/keys`: Create signing key

- request:
  ```json
  { "name": "<keyname>", "userid": "<user> <mail>" }
  ```
- response:
  ```json
  { "id": <i32_signing_key_id>, "name": "<keyname>", "public_armor": "-----BEGIN PGP …" }
  ```
- `name` is not actively used
- `userid` is used only during key creation

### `GET /v1/keys`: List signing keys

- response: 200 OK
  ```json
  [{ "id": <i32_signing_key_id>, "name": "<keyname>" }, ...]
  ```

### `GET /v1/keys/{id}`: Get signing key

- response: 200 OK, armor public key in raw text

### `DELETE /v1/keys/{id}`: Delete signing key

- response: 204 NO CONTENT | 404 NOT FOUND

## Schema Types

### `Repo`

```
#[derive(Clone, Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct Repo {
    pub id: i32,
    pub name: String,
    pub key_id: Option<i32>,
}
```
