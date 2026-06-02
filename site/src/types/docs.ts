export interface DocPage {
  id: string
  group: string
  groupIcon: string
  title: string
  /** Markdown-ish content string. Rendered by DocsContent with component detection. */
  content: string
  prev?: string   // page id of previous page
  next?: string   // page id of next page
}

export interface DocGroup {
  id: string
  icon: string
  label: string
  pages: string[]  // page ids in display order
}
