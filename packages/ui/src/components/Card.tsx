import React from 'react';

export interface CardProps {
  children: React.ReactNode;
  className?: string;
}

export const Card: React.FC<CardProps> = ({ children, className = '' }) => {
  return (
    <div className={`bg-white border-2 border-black brutalist-shadow-sm ${className}`}>
      {children}
    </div>
  );
};
