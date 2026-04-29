import { describe, expect, it } from 'vitest';
import {
  buildModelCatalog,
  deriveManualModels,
  normalizeModelList,
  prepareModelSelection,
} from './adminLlmProviderModels';

describe('admin LLM provider model helpers', () => {
  it('normalizes model lists by trimming and de-duplicating', () => {
    expect(
      normalizeModelList([
        ' gpt-4o ',
        '',
        'gpt-4o',
        'gpt-4o-mini',
        '  ',
        undefined,
      ]),
    ).toEqual(['gpt-4o', 'gpt-4o-mini']);
  });

  it('keeps the default model in the approved list', () => {
    expect(
      prepareModelSelection(['gpt-4o', 'gpt-4o-mini'], 'gpt-4.1'),
    ).toEqual({
      approvedModels: ['gpt-4o', 'gpt-4o-mini', 'gpt-4.1'],
      defaultModel: 'gpt-4.1',
    });
  });

  it('derives custom models from saved selections outside the preset catalog', () => {
    expect(
      deriveManualModels(
        ['gpt-4o', 'gpt-4o-mini'],
        ['gpt-4o', 'ft:gpt-4o:team/custom'],
        'ft:gpt-4o:team/custom',
      ),
    ).toEqual(['ft:gpt-4o:team/custom']);
  });

  it('builds a deduplicated catalog across preset, discovered, and custom models', () => {
    expect(
      buildModelCatalog({
        presetModels: ['gpt-4o', 'gpt-4o-mini'],
        discoveredModels: ['gpt-4o-mini', 'gpt-4.1'],
        manualModels: ['ft:gpt-4o:team/custom'],
        approvedModels: ['gpt-4o', 'ft:gpt-4o:team/custom', 'gpt-4.1'],
        defaultModel: 'gpt-4.1',
      }),
    ).toEqual({
      presetModels: ['gpt-4o', 'gpt-4o-mini'],
      discoveredModels: ['gpt-4.1'],
      customModels: ['ft:gpt-4o:team/custom'],
      allModels: [
        'gpt-4o',
        'gpt-4o-mini',
        'gpt-4.1',
        'ft:gpt-4o:team/custom',
      ],
    });
  });
});
