import type { Component } from 'vue'
import type { AppIconName } from './types'
import DashboardIcon from '~icons/lucide/layout-dashboard'
import ProfilesIcon from '~icons/lucide/server'
import ProxiesIcon from '~icons/lucide/network'
import BindingsIcon from '~icons/lucide/link'
import VisitorsIcon from '~icons/lucide/users'
import LogsIcon from '~icons/lucide/logs'
import StatusIcon from '~icons/lucide/activity'
import SettingsIcon from '~icons/lucide/settings'
import EnvironmentIcon from '~icons/lucide/boxes'
import LanguageIcon from '~icons/lucide/languages'
import LightIcon from '~icons/lucide/sun'
import DarkIcon from '~icons/lucide/moon'
import SystemIcon from '~icons/lucide/monitor'
import PaletteIcon from '~icons/lucide/palette'
import RefreshIcon from '~icons/lucide/refresh-cw'
import ReloadIcon from '~icons/lucide/rotate-ccw'
import SearchIcon from '~icons/lucide/search'
import FilterIcon from '~icons/lucide/list-filter'
import DownloadIcon from '~icons/lucide/download'
import UploadIcon from '~icons/lucide/upload'
import CopyIcon from '~icons/lucide/copy'
import PauseIcon from '~icons/lucide/pause'
import ResumeIcon from '~icons/lucide/play'
import FollowIcon from '~icons/lucide/arrow-down-to-line'
import ClearIcon from '~icons/lucide/trash-2'
import MoreIcon from '~icons/lucide/ellipsis'
import RunningIcon from '~icons/lucide/circle-check'
import StoppedIcon from '~icons/lucide/circle-stop'
import WarningIcon from '~icons/lucide/triangle-alert'
import ErrorIcon from '~icons/lucide/circle-alert'
import ClockIcon from '~icons/lucide/clock-3'
import CpuIcon from '~icons/lucide/cpu'
import MemoryIcon from '~icons/lucide/memory-stick'
import ArrowDownIcon from '~icons/lucide/arrow-down'
import ArrowUpIcon from '~icons/lucide/arrow-up'
import LogoutIcon from '~icons/lucide/log-out'
import ChevronDownIcon from '~icons/lucide/chevron-down'

export const appIcons = {
  dashboard: DashboardIcon,
  profiles: ProfilesIcon,
  proxies: ProxiesIcon,
  bindings: BindingsIcon,
  visitors: VisitorsIcon,
  logs: LogsIcon,
  status: StatusIcon,
  settings: SettingsIcon,
  environment: EnvironmentIcon,
  language: LanguageIcon,
  'theme-light': LightIcon,
  'theme-dark': DarkIcon,
  'theme-system': SystemIcon,
  palette: PaletteIcon,
  refresh: RefreshIcon,
  reload: ReloadIcon,
  search: SearchIcon,
  filter: FilterIcon,
  download: DownloadIcon,
  upload: UploadIcon,
  copy: CopyIcon,
  pause: PauseIcon,
  resume: ResumeIcon,
  'follow-tail': FollowIcon,
  clear: ClearIcon,
  more: MoreIcon,
  running: RunningIcon,
  stopped: StoppedIcon,
  warning: WarningIcon,
  error: ErrorIcon,
  clock: ClockIcon,
  cpu: CpuIcon,
  memory: MemoryIcon,
  'arrow-down': ArrowDownIcon,
  'arrow-up': ArrowUpIcon,
  logout: LogoutIcon,
  'chevron-down': ChevronDownIcon,
} satisfies Record<AppIconName, Component>
