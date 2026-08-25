export const promptCategories = ["review", "development", "other"] as const;

export type PromptCategory = (typeof promptCategories)[number];

export interface PromptTemplate {
  /** Stable client identifier. System prompts use a reserved `system-` prefix. */
  uuid: string;
  title: string;
  content: string;
  category: PromptCategory;
  sortOrder: number;
  system?: boolean;
  /** Cloud optimistic-concurrency revision; 0 means local-only and not yet created. */
  revision?: number;
  /** A Cloud tombstone was received; keep the local copy but never auto-upload it. */
  cloudDeletedLocallyRetained?: boolean;
}

export const MAX_CUSTOM_PROMPTS = 30;

export const promptCategoryLabels: Record<PromptCategory, string> = {
  review: "审查",
  development: "开发",
  other: "其他",
};

/** System presets are read-only and deliberately do not count towards the user's 30 items. */
export const systemPrompts: readonly PromptTemplate[] = [
  { uuid: "system-review-plan", title: "审查本次计划", category: "review", sortOrder: 0, system: true, content: "请审查本次计划：找出遗漏、风险、依赖关系和可以改进的地方，并按优先级给出建议。" },
  { uuid: "system-review-bugs", title: "检查潜在 Bug", category: "review", sortOrder: 1, system: true, content: "请审查本次修改是否存在 Bug、边界条件问题、回归风险或连带影响；给出可复现路径和修复建议。" },
  { uuid: "system-review-tests", title: "补足测试", category: "review", sortOrder: 2, system: true, content: "请分析本次改动的测试覆盖缺口，列出最有价值的单元测试、集成测试和回归用例。" },
  { uuid: "system-review-security", title: "安全审查", category: "review", sortOrder: 3, system: true, content: "请从认证、权限、输入处理、数据泄露和依赖风险角度审查这次改动。" },
  { uuid: "system-dev-implement", title: "实现方案", category: "development", sortOrder: 0, system: true, content: "请先阅读相关代码，给出简洁的实现方案，然后直接完成实现并运行必要的验证。" },
  { uuid: "system-dev-debug", title: "定位问题", category: "development", sortOrder: 1, system: true, content: "请定位这个问题的根因，说明证据和影响范围；在确认后给出最小、安全的修复并验证。" },
  { uuid: "system-dev-refactor", title: "重构代码", category: "development", sortOrder: 2, system: true, content: "请在不改变行为的前提下重构相关代码，提升可读性、边界清晰度和可测试性，并运行校验。" },
  { uuid: "system-dev-explain", title: "解释代码", category: "development", sortOrder: 3, system: true, content: "请结合调用链解释这段代码的职责、数据流、关键边界和可能的修改点。" },
  { uuid: "system-other-summary", title: "总结当前工作", category: "other", sortOrder: 0, system: true, content: "请总结当前工作进展：已完成内容、未完成事项、风险和下一步建议。" },
  { uuid: "system-other-commit", title: "生成提交说明", category: "other", sortOrder: 1, system: true, content: "请根据当前改动生成清晰、准确的提交说明，包含标题和必要的变更摘要。" },
];

export function canAddPrompt(customPromptCount: number): boolean {
  return customPromptCount < MAX_CUSTOM_PROMPTS;
}

export function isPromptCategory(value: string): value is PromptCategory {
  return (promptCategories as readonly string[]).includes(value);
}

/** Canonical picker order; keyboard and rendered category sections share this list. */
export function orderPromptsByCategory(prompts: readonly PromptTemplate[]): PromptTemplate[] {
  return promptCategories.flatMap((category) => prompts.filter((prompt) => prompt.category === category));
}

export type PromptLibraryChange = {
  type: "create" | "update" | "delete";
  uuid: string;
  baseRevision: number;
  title?: string;
  content?: string;
  category?: PromptCategory;
  sortOrder?: number;
};

/** Builds the per-item Cloud mutation batch from the last saved local state. */
export function buildPromptLibraryChanges(
  beforePrompts: readonly PromptTemplate[],
  afterPrompts: readonly PromptTemplate[],
  deleteMissing = true,
): PromptLibraryChange[] {
  const before = new Map(beforePrompts.map((prompt) => [prompt.uuid, prompt]));
  const after = new Map(afterPrompts.map((prompt) => [prompt.uuid, prompt]));
  return [
    ...[...after.values()].filter((prompt) => !prompt.cloudDeletedLocallyRetained).flatMap((prompt) => {
      const previous = before.get(prompt.uuid);
      if (previous && JSON.stringify({ ...previous, revision: undefined }) === JSON.stringify({ ...prompt, revision: undefined })) return [];
      return [{ type: (previous ? "update" : "create") as "create" | "update", uuid: prompt.uuid, baseRevision: previous?.revision ?? 0, title: prompt.title, content: prompt.content, category: prompt.category, sortOrder: prompt.sortOrder }];
    }),
    ...(deleteMissing ? [...before.values()].filter((prompt) => !after.has(prompt.uuid)).map((prompt) => ({ type: "delete" as const, uuid: prompt.uuid, baseRevision: prompt.revision ?? 0 })) : []),
  ];
}
