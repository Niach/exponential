-- EXP-672 item 5: the widget legacy top-level `position` launcher corner has
-- no read path any more (every reader takes `launcher`). Rows saved before
-- EXP-569 and never re-saved still carry only `position`; rewrite them into
-- the launcher shape the removed shim resolved them to (a fab at that corner
-- on both devices) so self-hosted instances keep their configured corner.
UPDATE widget_configs
SET form_config = jsonb_set(form_config, '{launcher}',
      jsonb_build_object(
        'desktop', jsonb_build_object('mode', 'fab', 'position', form_config->>'position'),
        'mobile',  jsonb_build_object('mode', 'fab', 'position', form_config->>'position')), true)
WHERE form_config ? 'position'
  AND NOT (form_config ? 'launcher')
  AND form_config->>'position' IN ('bottom-left', 'bottom-right');
