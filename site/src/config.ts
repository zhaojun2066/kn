// 官网部署构建时由手动流程从根 Cargo.toml 读取版本，本地开发用默认值
export const APP_VERSION = import.meta.env.VITE_APP_VERSION || 'dev'
export const APP_NAME = 'KN'

export const RELEASE_API_URL = 'https://api.knshark.com/api/v1/app-config/desktop-release'
export const IOS_APP_URL = import.meta.env.VITE_IOS_APP_URL || ''
