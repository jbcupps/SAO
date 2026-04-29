export interface ModelCatalog {
  presetModels: string[];
  discoveredModels: string[];
  customModels: string[];
  allModels: string[];
}

export function normalizeModelList(
  models: Array<string | null | undefined>,
): string[] {
  const normalized: string[] = [];

  for (const model of models) {
    const trimmed = model?.trim();
    if (!trimmed || normalized.includes(trimmed)) {
      continue;
    }
    normalized.push(trimmed);
  }

  return normalized;
}

export function prepareModelSelection(
  approvedModels: Array<string | null | undefined>,
  defaultModel?: string | null,
): { approvedModels: string[]; defaultModel: string } {
  const nextApprovedModels = normalizeModelList(approvedModels);
  const nextDefaultModel = defaultModel?.trim() ?? '';

  if (
    nextDefaultModel &&
    !nextApprovedModels.includes(nextDefaultModel)
  ) {
    nextApprovedModels.push(nextDefaultModel);
  }

  return {
    approvedModels: nextApprovedModels,
    defaultModel: nextDefaultModel,
  };
}

export function deriveManualModels(
  presetModels: Array<string | null | undefined>,
  approvedModels: Array<string | null | undefined>,
  defaultModel?: string | null,
): string[] {
  const presetSet = new Set(normalizeModelList(presetModels));
  const selection = prepareModelSelection(approvedModels, defaultModel);

  return selection.approvedModels.filter((model) => !presetSet.has(model));
}

export function buildModelCatalog({
  presetModels,
  discoveredModels,
  manualModels,
  approvedModels,
  defaultModel,
}: {
  presetModels: Array<string | null | undefined>;
  discoveredModels: Array<string | null | undefined>;
  manualModels: Array<string | null | undefined>;
  approvedModels: Array<string | null | undefined>;
  defaultModel?: string | null;
}): ModelCatalog {
  const normalizedPresetModels = normalizeModelList(presetModels);
  const presetSet = new Set(normalizedPresetModels);

  const normalizedDiscoveredModels = normalizeModelList(discoveredModels).filter(
    (model) => !presetSet.has(model),
  );
  const discoveredSet = new Set(normalizedDiscoveredModels);

  const normalizedCustomModels = normalizeModelList([
    ...manualModels,
    ...approvedModels,
    defaultModel,
  ]).filter(
    (model) => !presetSet.has(model) && !discoveredSet.has(model),
  );

  return {
    presetModels: normalizedPresetModels,
    discoveredModels: normalizedDiscoveredModels,
    customModels: normalizedCustomModels,
    allModels: [
      ...normalizedPresetModels,
      ...normalizedDiscoveredModels,
      ...normalizedCustomModels,
    ],
  };
}
