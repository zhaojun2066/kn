// 构建时从 tauri.conf.json 注入，本地开发用默认值
export const APP_VERSION = import.meta.env.VITE_APP_VERSION || 'dev'
export const APP_NAME = 'AI Profile Manager'

export const GITHUB_RELEASES = 'https://github.com/zhaojun2066/ai-profile-manager/releases'

// All download links point to GitHub Releases
export const DOWNLOAD_URL_ARM = GITHUB_RELEASES
export const DOWNLOAD_URL_INTEL = GITHUB_RELEASES
