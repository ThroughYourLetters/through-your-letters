import React from 'react';
import { Compass, PlusCircle, Map as MapIcon, Info } from 'lucide-react';
import { useUIStore } from '../../store/useUIStore';

type View = 'explore' | 'contribute' | 'map' | 'about';

interface NavigationProps {
  currentView: View;
  onViewChange: (view: View) => void;
}

const Navigation: React.FC<NavigationProps> = ({ currentView, onViewChange }) => {
  const { isMenuOpen, closeMenu } = useUIStore();
  
  const navItems: Array<{ view: View; icon: typeof Compass; label: string }> = [
    { view: 'explore', icon: Compass, label: 'Explore' },
    { view: 'contribute', icon: PlusCircle, label: 'Contribute' },
    { view: 'map', icon: MapIcon, label: 'Map' },
    { view: 'about', icon: Info, label: 'About' },
  ];

  const handleViewChange = (view: View) => {
    onViewChange(view);
    closeMenu();
  };

  return (
    <>
      {/* Desktop navigation */}
      <nav className="hidden md:block bg-slate-50 border-b-2 border-black">
        <div className="container mx-auto px-4 max-w-7xl">
          <div className="flex gap-2 py-4">
            {navItems.map((item) => {
              const Icon = item.icon;
              const isActive = currentView === item.view;
              
              return (
                <button
                  key={item.view}
                  onClick={() => handleViewChange(item.view)}
                  className={`flex items-center gap-2 px-4 py-2 text-xs font-black uppercase tracking-widest border-2 border-black transition-all ${
                    isActive ? 'bg-black text-white' : 'bg-white text-black hover:bg-slate-100'
                  }`}
                >
                  <Icon size={16} />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </div>
        </div>
      </nav>

      {/* Mobile navigation */}
      {isMenuOpen && (
        <div className="md:hidden fixed inset-0 z-40 bg-black/50" onClick={closeMenu}>
          <div 
            className="absolute right-0 top-0 h-full w-64 bg-white border-l-4 border-black p-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="space-y-2">
              {navItems.map((item) => {
                const Icon = item.icon;
                const isActive = currentView === item.view;
                
                return (
                  <button
                    key={item.view}
                    onClick={() => handleViewChange(item.view)}
                    className={`w-full flex items-center gap-3 px-4 py-3 text-sm font-black uppercase tracking-widest border-2 border-black ${
                      isActive ? 'bg-black text-white' : 'bg-white text-black'
                    }`}
                  >
                    <Icon size={20} />
                    <span>{item.label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </>
  );
};

export default Navigation;
