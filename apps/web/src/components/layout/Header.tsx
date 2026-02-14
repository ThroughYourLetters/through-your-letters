import React from 'react';
import { Menu } from 'lucide-react';
import { useUIStore } from '../../store/useUIStore';

const Header: React.FC = () => {
  const { toggleMenu } = useUIStore();

  return (
    <header className="sticky top-0 z-50 bg-white border-b-4 border-black">
      <div className="container mx-auto px-4 py-6 max-w-7xl">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 bg-rust border-2 border-black flex items-center justify-center">
              <span className="text-white text-2xl font-black">T</span>
            </div>
            <div>
              <h1 className="text-2xl font-black uppercase tracking-tighter leading-none">
                Through Your Letters
              </h1>
              <p className="text-[8px] font-bold uppercase tracking-widest text-slate-400">
                Bengaluru Street Typography Archive
              </p>
            </div>
          </div>
          
          <button 
            onClick={toggleMenu}
            className="md:hidden p-2 border-2 border-black bg-white hover:bg-slate-100"
          >
            <Menu size={24} />
          </button>
        </div>
      </div>
    </header>
  );
};

export default Header;
