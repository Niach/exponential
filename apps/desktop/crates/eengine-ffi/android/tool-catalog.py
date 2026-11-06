# #VAPP-265: writes src/main/kotlin/.../ToolCatalog.kt from capabilities.json.
#   python3 apps/desktop/crates/eengine-ffi/android/tool-catalog.py
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
fx = os.path.join(here, '../../../../../packages/eengine/fixtures/capabilities.json')
out = os.path.join(here, 'src/main/kotlin/at/exponential/engine/capabilities/ToolCatalog.kt')
d = json.load(open(fx))
caps = {}
for t in d['tools']:
    if t['runs'] != 'native':
        continue
    cap, tool = t['name'].split('.')
    caps.setdefault(cap, []).append({'name': tool, 'description': t['description'], 'inputSchema': t['inputSchema']})
lines = ['// #VAPP-265: GENERATED from packages/eengine/fixtures/capabilities.json `tools`',
         '// (the `runs: native` ones): every native tool as an MCP tool object with its',
         '// app-facing description and inputSchema (what an agent lists). ToolCatalogTest',
         '// locks it to the fixture; regenerate with tool-catalog.py when it changes.',
         'package at.exponential.engine.capabilities', '', 'internal object ToolCatalog {',
         '    /** `toolsJson()` per capability name. */',
         '    val tools: Map<String, String> = mapOf(']
for cap in sorted(caps):
    lines.append(f'        "{cap}" to """{json.dumps(caps[cap], ensure_ascii=False, separators=(",", ":"))}""",')
lines += ['    )', '}', '']
open(out, 'w').write('\n'.join(lines))
