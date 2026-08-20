-- EXP-569 removed the EXP-435 widget panel color overrides (one accent color
-- plus the dark/light/auto theme presets are the whole palette surface now,
-- with no legacy read path kept) — strip the dead keys from stored
-- form_config blobs so nothing lingers for future writers to trip over.
UPDATE widget_configs
SET form_config = form_config - 'backgroundColor' - 'textColor'
WHERE form_config ?| array['backgroundColor','textColor'];
