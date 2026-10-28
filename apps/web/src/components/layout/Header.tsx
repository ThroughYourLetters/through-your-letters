import React from 'react';
import { Menu } from 'lucide-react';
import { useUIStore } from '../../store/useUIStore';

const Header: React.FC = () => {
  const { toggleMenu } = useUIStore();

  return (
    <header className="sticky top-0 z-50 bg-surface border-b border-line">
      <div className="container mx-auto px-4 py-6 max-w-7xl">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 bg-terracotta border border-line flex items-center justify-center">
              <span className="text-white text-2xl font-semibold">T</span>
            </div>
            <div>
              <h1 className="text-2xl font-semibold uppercase tracking-tight leading-none">
                Through Your Letters
              </h1>
              <p className="text-xs font-bold uppercase tracking-widest text-stone-400">
                Bengaluru Street Typography Archive
              </p>
            </div>
          </div>
          
          <button 
            onClick={toggleMenu}
            className="md:hidden p-2 border border-line bg-surface hover:bg-stone-100"
          >
            <Menu size={24} />
          </button>
        </div>
      </div>
    </header>
  );
};

export default Header;
