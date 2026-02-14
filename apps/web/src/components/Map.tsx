import { useEffect, useRef } from 'react';

interface MapProps {
  letterings: Array<{
    id: string;
    location: { coordinates: [number, number] };
    thumbnail_urls: { small: string };
  }>;
}

export function Map({ letterings }: MapProps) {
  const mapRef = useRef<HTMLDivElement>(null);
  
  useEffect(() => {
    if (!mapRef.current || typeof window === 'undefined') return;
    
    // Use Leaflet (free, no API key needed)
    const L = (window as any).L;
    if (!L) return;
    
    const map = L.map(mapRef.current).setView([12.9716, 77.5946], 12);
    
    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      attribution: '© OpenStreetMap'
    }).addTo(map);
    
    letterings.forEach(item => {
      const [lng, lat] = item.location.coordinates;
      L.marker([lat, lng])
        .bindPopup(`<img src="${item.thumbnail_urls.small}" width="100"/>`)
        .addTo(map);
    });
    
    return () => map.remove();
  }, [letterings]);
  
  return <div ref={mapRef} className="w-full h-96 border-2 border-black" />;
}