# Vendored A2UI specification (basic catalog)

Source: https://github.com/google/A2UI, git tag `v0.9`, commit
`19919ef4c8ad3185867f70386fa4669284d7714c` (2026-04-06), directory
`specification/v0_9/json/`. Licence: Apache-2.0 (`LICENSE.txt` here, verbatim,
sha256 `f9946796a8a5bd5981565142fa42e3b0cc36aef6f80e86beaf5905b3c0fef1b5`).

The epic (VAPP-84) pins "v0.9.1": upstream publishes the closed v0.9 spec
under the single git tag `v0.9`; the catalog id inside it is
`https://a2ui.org/specification/v0_9/basic_catalog.json` and the wire
`version` constant is `"v0.9"`. VAPP-97 re-checks v1.0 on 2026-10-14.

Files are byte-for-byte copies (never edited; `vendor.test.ts` pins their
sha256):

| file | sha256 |
|---|---|
| `v0_9/basic_catalog.json` | `3dece1107de5cd9856cebf882f9e2c01ca3e2a5eba209b8ed585688765281e10` |
| `v0_9/common_types.json` | `899a9307d80fd9db71f640aec518f74defbe6dcd104f76d0e02eb83b79ea9f89` |
| `v0_9/server_to_client.json` | `e8c0d280ff9e338ceb26551cbf01cabeb50690f5ac941a27fef2ec0490682af2` |
| `v0_9/client_to_server.json` | `33f5aabc526f8a812a704c742832204b73b831105e073700e035d707dc7495cc` |
| `v0_9/client_capabilities.json` | `1178ec350a87313a49ced9dd00dd36a34fa3129fd67277091cecabb141b3ab84` |
| `v0_9/basic_catalog_rules.txt` | `ccd5b8158b7d459c26bb92dbe63fcb5d01a78311d39200951c5b062499b854df` |

`catalog/basic-map.json` maps every component, icon name and function of
this catalog onto the core catalog; the reducer turns an unmapped component
into the `Unknown` placeholder, never an error.
