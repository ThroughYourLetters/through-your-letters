import React from "react";
import { Link, NavLink } from "react-router-dom";
import { Menu, Plus, X } from "lucide-react";
import CitySelector from "./CitySelector";
import LogoMark from "./playful/LogoMark";
import { useAuthStore } from "../store/useAuthStore";
import { Locale, useLocaleStore } from "../store/useLocaleStore";

export const NAV_ITEMS = [
  { to: "/", label: "Explore", end: true },
  { to: "/map", label: "Map" },
  { to: "/community", label: "Community" },
  { to: "/about", label: "About" },
];

const Header: React.FC = () => {
  const { user, hydrated, hydrate, logout } = useAuthStore();
  const { locale, setLocale, t } = useLocaleStore();
  const [menuOpen, setMenuOpen] = React.useState(false);

  React.useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const closeMenu = () => setMenuOpen(false);

  const accountLinks = user ? (
    <>
      <NavLink to="/me/uploads" onClick={closeMenu} className={navClass}>
        {t("my_uploads")}
      </NavLink>
      <NavLink to="/me/notifications" onClick={closeMenu} className={navClass}>
        {t("alerts")}
      </NavLink>
      <button
        onClick={() => {
          logout();
          closeMenu();
        }}
        className="nav-link text-terracotta hover:text-terracotta-dark"
      >
        {t("logout")}
      </button>
    </>
  ) : (
    <NavLink to="/auth" onClick={closeMenu} className={navClass}>
      {t("signin")}
    </NavLink>
  );

  return (
    <header className="sticky top-0 z-50 border-b border-line/80 bg-paper/75 backdrop-blur-xl backdrop-saturate-150">
      <div className="page-container flex h-16 items-center gap-3 md:gap-6">
        <Link to="/" className="flex items-center gap-2.5 shrink-0" onClick={closeMenu}>
          <LogoMark />
          <span className="hidden sm:block font-display text-[17px] font-semibold tracking-tight leading-none">
            Through Your <em className="font-serif font-normal text-terracotta">Letters</em>
          </span>
        </Link>

        <nav className="hidden md:flex items-center gap-0.5 rounded-full border border-line bg-surface/70 p-1" aria-label="Main">
          {NAV_ITEMS.map(({ to, label, end }) => (
            <NavLink key={to} to={to} end={end} className={navClass}>
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-2">
          <CitySelector />
          <div className="hidden lg:flex items-center gap-1">
            {accountLinks}
            <LocaleSelect locale={locale} setLocale={setLocale} />
          </div>
          <Link to="/contribute" className="btn-primary hidden md:inline-flex !py-2">
            <Plus size={16} />
            Contribute
          </Link>
          <button
            onClick={() => setMenuOpen((v) => !v)}
            className="icon-btn lg:hidden"
            aria-label={menuOpen ? "Close menu" : "Open menu"}
            aria-expanded={menuOpen}
          >
            {menuOpen ? <X size={18} /> : <Menu size={18} />}
          </button>
        </div>
      </div>

      {menuOpen && (
        <div className="lg:hidden border-t border-line bg-paper animate-in">
          <div className="page-container flex flex-col gap-1 py-4">
            <div className="md:hidden flex flex-col gap-1 pb-3 mb-3 border-b border-line">
              {NAV_ITEMS.map(({ to, label, end }) => (
                <NavLink key={to} to={to} end={end} onClick={closeMenu} className={navClass}>
                  {label}
                </NavLink>
              ))}
            </div>
            <div className="flex flex-wrap items-center gap-1">
              {accountLinks}
              <LocaleSelect locale={locale} setLocale={setLocale} />
            </div>
          </div>
        </div>
      )}
    </header>
  );
};

const navClass = ({ isActive }: { isActive: boolean }) =>
  `nav-link ${isActive ? "nav-link-active" : ""}`;

const LocaleSelect: React.FC<{
  locale: Locale;
  setLocale: (locale: Locale) => void;
}> = ({ locale, setLocale }) => (
  <select
    value={locale}
    onChange={(e) => setLocale(e.target.value as Locale)}
    className="rounded-full border border-line bg-transparent px-3 py-1.5 text-xs font-medium text-muted outline-none cursor-pointer hover:border-ink"
    aria-label="Language"
  >
    <option value="en">EN</option>
    <option value="hi">HI</option>
  </select>
);

export default Header;
