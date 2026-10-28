import { scriptLabel } from "../types";
import React from 'react';
import { MapPin, Heart, MessageCircle, Eye } from 'lucide-react';
import type { Lettering } from '../types';

interface LetteringCardProps {
  lettering: Lettering;
  onClick?: () => void;
}

const LetteringCard: React.FC<LetteringCardProps> = ({ lettering, onClick }) => {
  const thumbnailUrl = lettering.thumbnail_urls?.medium || lettering.image_url;
  
  return (
    <div 
      className="group bg-surface border border-line shadow-soft hover:-translate-y-1 hover:shadow-lift transition-all cursor-pointer"
      onClick={onClick}
    >
      <div className="aspect-square bg-stone-100 border-b border-line overflow-hidden relative">
        {thumbnailUrl ? (
          <img 
            src={thumbnailUrl} 
            alt={`Lettering from ${lettering.pin_code}`}
            className="w-full h-full object-cover transition-all"
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-stone-300">
            <span className="font-display font-semibold text-6xl">?</span>
          </div>
        )}
        
        <div className="absolute top-2 left-2 bg-ink text-white text-xs font-semibold px-2 py-1 uppercase tracking-widest">
          {lettering.pin_code}
        </div>
        
        {lettering.status === 'PENDING' && (
          <div className="absolute top-2 right-2 bg-yellow-500 text-ink text-xs font-semibold px-2 py-1 uppercase tracking-widest">
            Processing
          </div>
        )}

        <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/70 to-transparent p-4 opacity-0 group-hover:opacity-100 transition-opacity">
          <button className="w-full bg-surface text-ink px-4 py-2 text-xs font-semibold uppercase tracking-widest flex items-center justify-center gap-2">
            <Eye size={14} />
            View Details
          </button>
        </div>
      </div>
      
      <div className="p-4 space-y-3">
        <div className="flex items-center gap-2 text-xs">
          <MapPin size={14} className="text-terracotta" />
          <span className="font-mono font-bold">{lettering.pin_code}</span>
        </div>
        
        {lettering.detected_text && (
          <p className="text-sm font-bold text-stone-700 line-clamp-2">
            "{lettering.detected_text}"
          </p>
        )}
        
        {lettering.ml_metadata && (
          <div className="flex gap-2 text-xs font-bold uppercase tracking-wider">
            {lettering.ml_metadata.style && (
              <span className="bg-stone-100 px-2 py-1 border border-line">{lettering.ml_metadata.style}</span>
            )}
            {lettering.ml_metadata.script && (
              <span className="bg-stone-100 px-2 py-1 border border-line">{scriptLabel(lettering.ml_metadata.script)}</span>
            )}
          </div>
        )}
        
        <div className="flex items-center justify-between pt-2 border-t border-stone-200">
          <div className="flex items-center gap-4 text-xs text-stone-500">
            <div className="flex items-center gap-1">
              <Heart size={14} />
              <span>{lettering.likes_count || 0}</span>
            </div>
            <div className="flex items-center gap-1">
              <MessageCircle size={14} />
              <span>{lettering.comments_count || 0}</span>
            </div>
          </div>
          
          <div className="text-xs font-bold text-stone-400 uppercase tracking-widest">
            @{lettering.contributor_tag}
          </div>
        </div>
      </div>
    </div>
  );
};

export default LetteringCard;
