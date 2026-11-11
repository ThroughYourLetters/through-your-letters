-- Read public rows without joining letterings to a second copy of itself.
CREATE OR REPLACE VIEW discoverable_letterings AS
SELECT l.*
FROM letterings l
JOIN cities c ON c.id = l.city_id
LEFT JOIN region_policies rp ON rp.country_code = c.country_code
WHERE l.status = 'APPROVED'
  AND COALESCE(rp.discoverability_enabled, true);

-- Counter and moderation updates do not change searchable words.
DROP TRIGGER letterings_tsv_update ON letterings;
CREATE TRIGGER letterings_tsv_update
    BEFORE INSERT OR UPDATE OF detected_text, description ON letterings
    FOR EACH ROW EXECUTE FUNCTION update_lettering_tsv();
