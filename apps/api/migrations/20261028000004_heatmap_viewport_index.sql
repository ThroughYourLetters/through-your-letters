-- Planar bounds also cover wide world views.
CREATE INDEX letterings_heatmap_viewport ON letterings USING GIST ((location::geometry))
    WHERE status='APPROVED' AND location_source='postal_code';
