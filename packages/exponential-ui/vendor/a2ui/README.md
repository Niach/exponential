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


## A2UI 1.0 (VAPP-289)

Source: https://github.com/a2ui-project/a2ui (formerly google/A2UI), branch
`main` at commit `1e9ed04b5585721ac6df8313030f9bfb42ccc6fa` (2026-11-06):
`specification/v1_0/json/*.json` → `v1_0/json/`,
`catalogs/basic/v1/catalog.json` → `v1_0/catalogs/basic/v1/`,
`specification/v1_0/test/cases/*.json` + `test/testing_catalog.json` →
`v1_0/test/`, `specification/v1_0/docs/a2ui_protocol.md` → `v1_0/docs/`.
The wire `version` is `"1.0"`; the basic catalog id is
`https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json`. `v0_9/`
stays as `upgrade09`'s reference (round 9 contract §14).

| file | sha256 |
|---|---|
| `v1_0/catalogs/basic/v1/catalog.json` | `c9144af19eac9be3256315663dd50b9f010b34beef7a705d17fff50ce0d6bd47` |
| `v1_0/docs/a2ui_protocol.md` | `51fca71060af3666a803981e9819428f0c8e3ba01e3a6081891dc3276093944c` |
| `v1_0/json/agent_capabilities.json` | `6a4282dfb26af95a73d960572a31b91c5d5c89d1f9e54cbb8d24c599afcbc3a2` |
| `v1_0/json/agent_to_renderer.json` | `b1cacb676785a3f25b7429f2fa3a72c1dd554738f2c2d3f00aaa41fcd45b360c` |
| `v1_0/json/agent_to_renderer_list.json` | `3d3fef882fe5cc22932d3a91713defaeeb329126200954c06bf9f2c56df51c56` |
| `v1_0/json/agent_to_renderer_list_wrapper.json` | `e306af2521e731db673415115d55b3d0e66453945c071e34298b4cb1dff88014` |
| `v1_0/json/catalog_definition.json` | `836a48b2deb7ae76c3b68cb4e79c2faa24cba36d4d5396474894796797364d44` |
| `v1_0/json/common_types.json` | `8225a34c954537abef057f7cd599fa59361fce2dbbe89349ddf3fe49cc914c15` |
| `v1_0/json/renderer_capabilities.json` | `3ebd2889764d892df16d034643ecd9e3a3fc02f298fa05577b95d9e287e703de` |
| `v1_0/json/renderer_data_model.json` | `16f7d6a9aa31fed6fd3a1deb924db14a759ff6514bfe386a0dca8c48389cdbc8` |
| `v1_0/json/renderer_to_agent.json` | `58ecde02101f0f37566b5e238dc6ef6af628c3ff312fce0e17ab7670b7215465` |
| `v1_0/json/renderer_to_agent_list.json` | `98899d7d16222207af3bfd9aab12aa7b8f68dc8b5267dcf76d867236235ec5d8` |
| `v1_0/json/renderer_to_agent_list_wrapper.json` | `a0dbcaf2340def4485c287c19888aa81041ddee75e5717129457d4a39fbcc593` |
| `v1_0/json/sample.json` | `7df58a955dfcd5241bfd5631df8fd291eb02d66f2f9a12304d5395517d64202f` |
| `v1_0/test/cases/agent_capabilities.json` | `9154ec096b868e627a00df2ef9178a261a06b9ec1c1776a2120abba46c60de31` |
| `v1_0/test/cases/button_checks.json` | `c5b90d81d6227f8140d30f9866420d09291ffd12db3007cd7772d690221f4eea` |
| `v1_0/test/cases/call_agent_function_message.json` | `dc752cda346a64cfd48bdeb850c954721132e94b39e61f4ddc303482ee463573` |
| `v1_0/test/cases/call_function_message.json` | `9854d2b3c75638f228d398cd77ff1fd4a837889dedb2ce333818569bc36e9fba` |
| `v1_0/test/cases/catalog_schema_relaxation.json` | `071692c76e2c21e8b11bdd4208b95a7896ec46359a5e7a7ec6c767a7eb2170be` |
| `v1_0/test/cases/checkable_components.json` | `3b0c3632a82e68fe138a2d383c70902863efe3f750646846c44c8b48ba81eb44` |
| `v1_0/test/cases/composition_constraints.json` | `469ede847664c349d2801ae19e09a5f417b53f96c4a165804e8c4a8b5b7b05a4` |
| `v1_0/test/cases/contact_form_example_test.json` | `4c3cec7952680f7be008ecdde3e8d0cfe37a671ed372284b8e59f7d596adb2eb` |
| `v1_0/test/cases/dynamic_value_validation.json` | `7b6ebaae9257c66b624cc5c79312f88154bd3c8b2b1e8af512556fc6a58ad065` |
| `v1_0/test/cases/function_catalog_validation.json` | `92e796d5edd818e40c414436084b8545d6d4189f16a76ab51e5f1d85c0294504` |
| `v1_0/test/cases/function_definitions.json` | `c19223bf12c88bbd46eeb53d3298f235611265146e74b4b4422ba86284e9f288` |
| `v1_0/test/cases/function_response.json` | `d3784912a406e5626530583d5a1698b1ec265e33bc3d6965df5933710c99131d` |
| `v1_0/test/cases/icon_checks.json` | `1c26b1f353212fec66e9482c0f9a17db9051edf19f63237bc29fc829dbe3d58f` |
| `v1_0/test/cases/initial_state_validation.json` | `50055adcbbcaa6990383921c698180948b009559135600f33714a83bc9ef2297` |
| `v1_0/test/cases/renderer_messages.json` | `a5947e13e6b9fd99a33d5c8e4dd1410aa11bc2b0df4e29d9df22a074a54365ac` |
| `v1_0/test/cases/tabs_checks.json` | `390e42fab63f9c2918f094177b32d88c2d50661d71737abe77a03969618f451b` |
| `v1_0/test/cases/text_variants.json` | `c104f9ffd0ea02225d247ee50fd77f2b9b00eeb9c297e69a82d297237f39bf80` |
| `v1_0/test/testing_catalog.json` | `12a5f4dc96b7973596035f92ab5af2d6cd6be1045d32a71a72e55426a4d54807` |

## Unicode 17.0.0

`../unicode/DerivedCoreProperties.txt` is
https://www.unicode.org/Public/17.0.0/ucd/DerivedCoreProperties.txt, byte for
byte (Unicode licence, https://www.unicode.org/terms_of_use.html). The
generator reads its `XID_Start` / `XID_Continue` ranges into the generated
`xid` tables (round 9 contract §4).

| file | sha256 |
|---|---|
| `../unicode/DerivedCoreProperties.txt` | `24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08` |
