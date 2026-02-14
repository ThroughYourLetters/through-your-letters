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
      className="group bg-white border-2 border-black brutalist-shadow-sm hover:-translate-y-1 hover:brutalist-shadow-lg transition-all cursor-pointer"
      onClick={onClick}
    >
      <div className="aspect-square bg-slate-100 border-b-2 border-black overflow-hidden relative">
        {thumbnailUrl ? (
          <img 
            src={thumbnailUrl} 
            alt={`Lettering from ${lettering.pin_code}`}
            className="w-full h-full object-cover grayscale group-hover:grayscale-0 transition-all"
            loading="lazy"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-slate-300">
            <span className="text-6xl font-black">?</span>
          </div>
        )}
        
        <div className="absolute top-2 left-2 bg-black text-white text-[7px] font-black px-2 py-1 uppercase tracking-widest">
          {lettering.pin_code}
        </div>
        
        {lettering.status === 'PENDING' && (
          <div className="absolute top-2 right-2 bg-yellow-500 text-black text-[7px] font-black px-2 py-1 uppercase tracking-widest">
            Processing
          </div>
        )}

        <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/70 to-transparent p-4 opacity-0 group-hover:opacity-100 transition-opacity">
          <button className="w-full bg-white text-black px-4 py-2 text-xs font-black uppercase tracking-widest flex items-center justify-center gap-2">
            <Eye size={14} />
            View Details
          </button>
        </div>
      </div>
      
      <div className="p-4 space-y-3">
        <div className="flex items-center gap-2 text-xs">
          <MapPin size={14} className="text-rust" />
          <span className="font-mono font-bold">{lettering.pin_code}</span>
        </div>
        
        {lettering.detected_text && (
          <p className="text-sm font-bold text-slate-700 line-clamp-2">
            "{lettering.detected_text}"
          </p>
        )}
        
        {lettering.ml_metadata && (
          <div className="flex gap-2 text-[8px] font-bold uppercase tracking-wider">
            {lettering.ml_metadata.style && (
              <span className="bg-slate-100 px-2 py-1 border border-black">{lettering.ml_metadata.style}</span>
            )}
            {lettering.ml_metadata.script && (
              <span className="bg-slate-100 px-2 py-1 border border-black">{lettering.ml_metadata.script}</span>
            )}
          </div>
        )}
        
        <div className="flex items-center justify-between pt-2 border-t border-slate-200">
          <div className="flex items-center gap-4 text-xs text-slate-500">
            <div className="flex items-center gap-1">
              <Heart size={14} />
              <span>{lettering.likes_count || 0}</span>
            </div>
            <div className="flex items-center gap-1">
              <MessageCircle size={14} />
              <span>{lettering.comments_count || 0}</span>
            </div>
          </div>
          
          <div className="text-[8px] font-bold text-slate-400 uppercase tracking-widest">
            @{lettering.contributor_tag}
          </div>
        </div>
      </div>
    </div>
  );
};

export default LetteringCard;
