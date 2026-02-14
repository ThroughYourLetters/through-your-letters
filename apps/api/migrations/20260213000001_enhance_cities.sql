-- Add cover_image_url column if not exists
ALTER TABLE cities ADD COLUMN IF NOT EXISTS cover_image_url TEXT;

-- Update Bengaluru with coordinates
UPDATE cities SET center_lat = 12.9716, center_lng = 77.5946, default_zoom = 12, description = 'The Garden City — where hand-painted signs mix with tech-park typography', is_active = true WHERE name = 'Bengaluru';

-- Seed additional Indian cities
INSERT INTO cities (id, name, country_code, center_lat, center_lng, default_zoom, description, is_active)
VALUES
    ('0194f123-4567-7abc-8def-100000000001', 'Mumbai', 'IN', 19.0760, 72.8777, 12, 'Maximum city, maximum lettering — from Bollywood posters to Dabbawalas', false),
    ('0194f123-4567-7abc-8def-100000000002', 'Delhi', 'IN', 28.6139, 77.2090, 12, 'Capital typography — Mughal calligraphy meets modern signage', false),
    ('0194f123-4567-7abc-8def-100000000003', 'Chennai', 'IN', 13.0827, 80.2707, 12, 'Tamil script traditions and colonial-era lettering heritage', false),
    ('0194f123-4567-7abc-8def-100000000004', 'Kolkata', 'IN', 22.5726, 88.3639, 12, 'Bengali calligraphy and hand-painted cinema hoardings', false),
    ('0194f123-4567-7abc-8def-100000000005', 'Hyderabad', 'IN', 17.3850, 78.4867, 12, 'Telugu and Urdu scripts side by side in the City of Pearls', false),
    ('0194f123-4567-7abc-8def-100000000006', 'Pune', 'IN', 18.5204, 73.8567, 12, 'Marathi lettering from Shaniwar Wada to university campuses', false)
ON CONFLICT (name) DO NOTHING;
