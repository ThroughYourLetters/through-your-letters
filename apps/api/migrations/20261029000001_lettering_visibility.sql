-- Public discovery has one policy, including counts and linked uploads.
CREATE VIEW discoverable_letterings AS
SELECT l.id
FROM letterings l
JOIN cities c ON c.id = l.city_id
LEFT JOIN region_policies rp ON rp.country_code = c.country_code
WHERE l.status = 'APPROVED'
  AND COALESCE(rp.discoverability_enabled, true);

-- Owner previews do not grant public discoverability to an upload or its links.
CREATE FUNCTION readable_letterings(requester UUID)
RETURNS SETOF letterings
LANGUAGE sql STABLE
AS $$
    SELECT l.*
    FROM letterings l
    LEFT JOIN discoverable_letterings visible ON visible.id = l.id
    WHERE l.user_id = requester OR visible.id IS NOT NULL
$$;
