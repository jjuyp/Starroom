export const goldenTags = [
  'raw', 'camera-color', 'tone', 'wb', 'curve', 'color', 'grading', 'detail',
  'optics', 'geometry', 'mask', 'portrait', 'skin', 'ai', 'night', 'high-iso',
  'neon', 'landscape', 'hdr',
]

export const targets = {
  library: {
    rust: [['test', '--locked', '-p', 'starroom-library']],
    web: ['src/nativeRender.test.ts'],
    golden: [],
  },
  history: {
    rust: [['test', '--locked', '-p', 'starroom-history']],
    web: ['src/editorState.test.ts', 'src/nativeRender.test.ts'],
    golden: [],
  },
  color: {
    rust: [['test', '--locked', '-p', 'starroom-color', '-p', 'starroom-color-management', '-p', 'starroom-grading', '-p', 'starroom-reference', '-p', 'starroom-look']],
    web: ['src/imagePipeline.test.ts', 'src/nativeRender.test.ts'],
    golden: ['color', 'camera-color'],
  },
  tone: {
    rust: [
      ['test', '--locked', '-p', 'starroom-color', 'tone'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'shadow'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'preview_and_export'],
    ],
    web: ['src/imagePipeline.test.ts', 'src/nativeRender.test.ts'],
    golden: ['tone', 'portrait', 'night', 'hdr'],
  },
  curve: {
    rust: [
      ['test', '--locked', '-p', 'starroom-color', 'curve'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'curve'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'portrait_and_gradient'],
      ['test', '--locked', '-p', 'starroom-project', 'adjustment_state'],
    ],
    web: ['src/imagePipeline.test.ts', 'src/nativeRender.test.ts', 'src/editorState.test.ts'],
    golden: ['curve', 'portrait'],
  },
  raw: {
    rust: [
      ['test', '--locked', '-p', 'starroom-raw'],
      ['test', '--locked', '-p', 'starroom-pipeline', '--test', 'raw_shared_graph'],
    ],
    web: [],
    golden: ['raw', 'camera-color'],
  },
  detail: {
    rust: [['test', '--locked', '-p', 'starroom-detail', '-p', 'starroom-heal', '-p', 'starroom-portrait', '-p', 'starroom-ai-denoise', '-p', 'starroom-look']],
    web: ['src/imagePipeline.test.ts'],
    golden: ['detail', 'high-iso'],
  },
  optics: {
    rust: [['test', '--locked', '-p', 'starroom-optics']],
    web: [],
    golden: ['optics'],
  },
  geometry: {
    rust: [['test', '--locked', '-p', 'starroom-geometry']],
    web: [],
    golden: ['geometry', 'landscape'],
  },
  gpu: {
    rust: [
      ['test', '--locked', '-p', 'starroom-render', 'gpu'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'm12_gpu'],
    ],
    web: ['src/nativeRender.test.ts'],
    golden: ['raw', 'tone', 'curve', 'color', 'grading', 'detail', 'portrait', 'skin', 'neon', 'landscape', 'hdr'],
  },
  tiles: {
    rust: [['test', '--locked', '-p', 'starroom-render', 'scheduler']],
    web: ['src/nativeRender.test.ts'],
    golden: ['raw', 'detail', 'geometry', 'portrait', 'landscape', 'hdr'],
  },
  layers: {
    rust: [['test', '--locked', '-p', 'starroom-pipeline', 'm14_layer'], ['test', '--locked', '-p', 'starroom-project', 'layer']],
    web: ['src/nativeRender.test.ts', 'src/editorState.test.ts'],
    golden: ['portrait', 'night', 'hdr'],
  },
  masks: {
    rust: [['test', '--locked', '-p', 'starroom-project', 'mask'], ['test', '--locked', '-p', 'starroom-pipeline', 'm15_'], ['test', '--locked', '-p', 'starroom-render']],
    web: ['src/editorState.test.ts', 'src/nativeRender.test.ts'],
    golden: ['mask'],
  },
  portrait: {
    rust: [
      ['test', '--locked', '-p', 'starroom-portrait', '-p', 'starroom-detail'],
      ['test', '--locked', '-p', 'starroom-project', 'm16_'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'm16_'],
    ],
    web: ['src/nativeRender.test.ts'],
    golden: ['portrait', 'skin', 'mask', 'geometry'],
  },
  ai: {
    rust: [
      ['test', '--locked', '-p', 'starroom-advisor'],
      ['test', '--locked', '-p', 'starroom-portrait', 'm17_'],
      ['test', '--locked', '-p', 'starroom-heal', 'm18_'],
      ['test', '--locked', '-p', 'starroom-advisor', 'm19_'],
      ['test', '--locked', '-p', 'starroom-portrait', 'm20_'],
      ['test', '--locked', '-p', 'starroom-project', 'm20_'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'm20_'],
      ['test', '--locked', '-p', 'starroom-ai-denoise'],
      ['test', '--locked', '-p', 'starroom-reference'],
      ['test', '--locked', '-p', 'starroom-look'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'm21_'],
      ['test', '--locked', '-p', 'starroom-pipeline', 'm23_'],
    ],
    web: ['src/nativeRender.test.ts'],
    golden: ['ai', 'portrait', 'skin', 'mask'],
  },
  web: {
    rust: [],
    web: ['src'],
    golden: ['tone', 'curve'],
  },
}

export const sharedGraphRust = [
  ['test', '--locked', '-p', 'starroom-pipeline'],
  ['test', '--locked', '-p', 'starroom-render'],
]
