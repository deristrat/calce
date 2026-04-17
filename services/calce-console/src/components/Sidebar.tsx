import { NavLink } from 'react-router'
import { useAuth } from '../auth/AuthContext'
import {
  IconDashboard,
  IconBuilding,
  IconUsers,
  IconChart,
  IconCurrency,
  IconActivity,
  IconDatabase,
  IconPalette,
  IconServer,
  IconSettings,
  IconLogout,
} from './icons'
import ThemeToggle from './ThemeToggle'

function Sidebar() {
  const { user, logout } = useAuth()

  const linkClass = ({ isActive }: { isActive: boolean }) =>
    ['ds-sidebar__item', isActive && 'ds-sidebar__item--active'].filter(Boolean).join(' ')

  return (
    <aside className="ds-sidebar">
      <div className="ds-sidebar__logo">Calce</div>
      <nav className="ds-sidebar__nav">
        <div className="ds-sidebar__section">
          <div className="ds-sidebar__section-title">Overview</div>
          <NavLink to="/dashboard" className={linkClass}>
            <IconDashboard /> Dashboard
          </NavLink>
        </div>
        <div className="ds-sidebar__section">
          <div className="ds-sidebar__section-title">Data</div>
          <NavLink to="/organizations" className={linkClass}>
            <IconBuilding /> Organizations
          </NavLink>
          <NavLink to="/users" className={linkClass}>
            <IconUsers /> Users
          </NavLink>
          <NavLink to="/instruments" className={linkClass}>
            <IconChart /> Instruments
          </NavLink>
          <NavLink to="/fx-rates" className={linkClass}>
            <IconCurrency /> FX Rates
          </NavLink>
        </div>
        <div className="ds-sidebar__section">
          <div className="ds-sidebar__section-title">Dev Tools</div>
          <NavLink to="/simulator" className={linkClass}>
            <IconActivity /> Price Simulator
          </NavLink>
          <NavLink to="/update-simulator" className={linkClass}>
            <IconDatabase /> Update Simulator
          </NavLink>
        </div>
        <div className="ds-sidebar__section">
          <div className="ds-sidebar__section-title">System</div>
          <NavLink to="/system/overview" className={linkClass}>
            <IconServer /> Overview
          </NavLink>
          <NavLink to="/system/config" className={linkClass}>
            <IconSettings /> Config
          </NavLink>
        </div>
        <div className="ds-sidebar__section">
          <div className="ds-sidebar__section-title">Design</div>
          <NavLink to="/design/system" className={linkClass}>
            <IconPalette /> System
          </NavLink>
          <NavLink to="/design/components" className={linkClass}>
            <IconDashboard /> Components
          </NavLink>
          <NavLink to="/design/examples" className={linkClass}>
            <IconChart /> Examples
          </NavLink>
        </div>
      </nav>
      <div className="ds-sidebar__user">
        <ThemeToggle className="ds-sidebar__item ds-sidebar__user-action" />
        <span className="ds-sidebar__user-name">
          {user?.email || 'Admin'}
        </span>
        <button className="ds-sidebar__item ds-sidebar__user-action" onClick={logout}>
          <IconLogout />
        </button>
      </div>
    </aside>
  )
}

export default Sidebar
