import { readFileSync } from 'node:fs'
import ts from 'typescript'
import { parse, type Rule } from 'postcss'
import { describe, expect, it } from 'vitest'
import { commandCatalog } from '../../src/commands'

// These are structural guards on the real production TSX, not substitutes for native
// interaction or screenshot acceptance. Parsing the syntax tree makes the contract
// independent of whitespace, translated labels and JSX formatting.
const source = ts.createSourceFile('App.tsx', readFileSync(new URL('../../src/App.tsx', import.meta.url), 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
const styleRules: Rule[] = []
for (const file of ['styles.css', 'glass.css']) {
  parse(readFileSync(new URL(`../../src/${file}`, import.meta.url), 'utf8')).walkRules((rule) => { styleRules.push(rule) })
}

function descendants(root: ts.Node, predicate: (node: ts.Node) => boolean): ts.Node[] {
  const found: ts.Node[] = []
  const visit = (node: ts.Node) => { if (predicate(node)) found.push(node); ts.forEachChild(node, visit) }
  visit(root)
  return found
}

function opening(node: ts.Node): ts.JsxOpeningLikeElement | null {
  return ts.isJsxElement(node) ? node.openingElement : ts.isJsxSelfClosingElement(node) ? node : null
}

function attribute(node: ts.Node, name: string): ts.JsxAttribute | undefined {
  return opening(node)?.attributes.properties.find((entry): entry is ts.JsxAttribute => ts.isJsxAttribute(entry) && entry.name.getText(source) === name)
}

function hasClass(node: ts.Node, name: string): boolean {
  const initializer = attribute(node, 'className')?.initializer
  return Boolean(initializer && ts.isStringLiteral(initializer) && initializer.text.split(/\s+/).includes(name))
}

function withinClass(node: ts.Node, name: string): boolean {
  for (let parent: ts.Node | undefined = node; parent; parent = parent.parent) if (hasClass(parent, name)) return true
  return false
}

function calls(root: ts.Node, name: string): ts.CallExpression[] {
  return descendants(root, (node) => ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === name) as ts.CallExpression[]
}

function identifier(root: ts.Node, name: string): boolean {
  return descendants(root, (node) => ts.isIdentifier(node) && node.text === name).length > 0
}

function functionBody(name: string): ts.FunctionDeclaration {
  const declaration = descendants(source, (node) => ts.isFunctionDeclaration(node) && node.name?.text === name)[0]
  expect(declaration, `production function ${name} must exist`).toBeDefined()
  return declaration as ts.FunctionDeclaration
}

function currentGuard(root: ts.Node): boolean {
  return calls(root, 'current').length > 0
}

function rulesFor(selector: string, width = 1920): Rule[] {
  return styleRules.filter((rule) => {
    if (!rule.selectors.includes(selector)) return false
    for (let parent = rule.parent; parent; parent = parent.parent) {
      if (parent.type !== 'atrule' || parent.name !== 'media') continue
      const maximum = /max-width\s*:\s*(\d+)px/.exec(parent.params)
      const minimum = /min-width\s*:\s*(\d+)px/.exec(parent.params)
      if (maximum && width > Number(maximum[1]) || minimum && width < Number(minimum[1])) return false
    }
    return true
  })
}

function winningValue(selector: string, property: string, width = 1920): string | undefined {
  const values: string[] = []
  for (const rule of rulesFor(selector, width)) rule.walkDecls(property, (declaration) => { values.push(declaration.value) })
  return values.at(-1)
}

describe('production workspace UI structure', () => {
  it('owns Presets and History in the left sidebar only', () => {
    for (const tab of ['presets', 'history']) {
      const branches = descendants(source, (node) => ts.isBinaryExpression(node)
        && node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken
        && ts.isIdentifier(node.left) && node.left.text === 'developTab'
        && ts.isStringLiteral(node.right) && node.right.text === tab)
      expect(branches, `${tab} must have exactly one rendered tab body`).toHaveLength(1)
      expect(withinClass(branches[0], 'library-panel')).toBe(true)
      expect(withinClass(branches[0], 'canvas-area')).toBe(false)
    }
    const canvas = descendants(source, (node) => hasClass(node, 'canvas-area'))
    expect(canvas).toHaveLength(1)
    for (const callback of ['loadCurvePreset', 'loadLookWorkflow', 'saveLookWorkflow', 'createSnapshot', 'restoreSnapshot']) {
      expect(identifier(canvas[0], callback), `${callback} must not be duplicated in the central photo workspace`).toBe(false)
    }
    expect(identifier(canvas[0], 'nativeHistory')).toBe(false)
  })

  it('binds the inspector disclosure to both accessibility state and actual visibility', () => {
    const toggles = descendants(source, (node) => hasClass(node, 'inspector-toggle'))
    expect(toggles).toHaveLength(1)
    const toggle = toggles[0]
    const expanded = attribute(toggle, 'aria-expanded')?.initializer
    expect(expanded && identifier(expanded, 'collapsed')).toBe(true)
    const click = attribute(toggle, 'onClick')?.initializer
    expect(click && calls(click, 'setCollapsed')).toHaveLength(1)
    const inspector = descendants(source, (node) => ts.isFunctionDeclaration(node) && node.name?.text === 'Inspector')[0]
    expect(inspector).toBeDefined()
    const hidden = descendants(inspector, (node) => Boolean(attribute(node, 'hidden')?.initializer && identifier(attribute(node, 'hidden')!.initializer!, 'collapsed')))
    expect(hidden).toHaveLength(1)
  })

  it('resets primary inspector scroll only when the active tool changes and keeps its sticky title readable', () => {
    const containers = descendants(source, (node) => {
      const value = attribute(node, 'className')?.initializer
      return Boolean(value && identifier(value, 'tool') && attribute(node, 'ref')?.initializer
        && identifier(attribute(node, 'ref')!.initializer!, 'inspectorScrollRef'))
    })
    expect(containers).toHaveLength(1)
    const effects = calls(source, 'useLayoutEffect').filter((call) => identifier(call.arguments[0], 'inspectorScrollRef'))
    expect(effects).toHaveLength(1)
    const dependencies = effects[0].arguments[1]
    expect(ts.isArrayLiteralExpression(dependencies)).toBe(true)
    expect((dependencies as ts.ArrayLiteralExpression).elements.map((entry) => entry.getText(source))).toEqual(['tool'])
    const resets = descendants(effects[0].arguments[0], (node) => ts.isBinaryExpression(node)
      && node.operatorToken.kind === ts.SyntaxKind.EqualsToken
      && ts.isPropertyAccessExpression(node.left) && node.left.name.text === 'scrollTop'
      && ts.isNumericLiteral(node.right) && node.right.text === '0')
    expect(resets).toHaveLength(1)
    const background = winningValue('.theme-dark .inspector-toggle', 'background')
    expect(background).toMatch(/^rgba\(/)
    const alpha = Number(background!.match(/,\s*([.\d]+)\)$/)?.[1])
    expect(alpha).toBeGreaterThanOrEqual(.9)
    expect(winningValue('.theme-dark .inspector-head', 'backdrop-filter')).toBe('blur(12px)')
  })

  it('hydrates native history and snapshots through the local-tone wire adapter instead of casting raw layers into UI state', () => {
    const hydrator = descendants(source, (node) => ts.isVariableDeclaration(node)
      && ts.isIdentifier(node.name) && node.name.text === 'applyNativeHistoryState')[0] as ts.VariableDeclaration
    expect(hydrator?.initializer).toBeDefined()
    expect(calls(hydrator, 'fromNativeSettings')).toHaveLength(1)
    const layers = descendants(hydrator, (node) => ts.isPropertyAssignment(node)
      && node.name.getText(source) === 'layers') as ts.PropertyAssignment[]
    expect(layers).toHaveLength(1)
    expect(ts.isPropertyAccessExpression(layers[0].initializer)).toBe(true)
    expect(identifier(layers[0].initializer, 'mapped')).toBe(true)
    expect(identifier(layers[0].initializer, 'state')).toBe(false)
    const mask = descendants(hydrator, (node) => ts.isPropertyAssignment(node)
      && node.name.getText(source) === 'mask') as ts.PropertyAssignment[]
    expect(mask).toHaveLength(1)
    expect(identifier(mask[0].initializer, 'mapped')).toBe(true)
    expect(identifier(mask[0].initializer, 'photo')).toBe(true)
  })

  it('keeps every command palette entry connected to the shared command dispatcher', () => {
    const dispatch = descendants(source, (node) => ts.isFunctionDeclaration(node) && node.name?.text === 'executeCommand')[0]
    expect(dispatch).toBeDefined()
    const cases = descendants(dispatch, ts.isCaseClause).map((node) => (node as ts.CaseClause).expression).filter(ts.isStringLiteral).map((node) => node.text)
    for (const command of commandCatalog) {
      // Ratings intentionally share the validated numeric dispatch rather than six action paths.
      if (/^rate[0-5]$/.test(command.id)) {
        expect(calls(dispatch, 'updateLibraryWorkflow')).not.toHaveLength(0)
        expect(descendants(dispatch, ts.isDefaultClause)).toHaveLength(1)
      } else {
        expect(cases, `command ${command.id} lacks its production dispatcher case`).toContain(command.id)
      }
    }
  })

  it('has no inert native buttons in the production component tree', () => {
    const buttons = descendants(source, (node) => opening(node)?.tagName.getText(source) === 'button')
    expect(buttons.length).toBeGreaterThan(30)
    const inert = buttons.filter((node) => !attribute(node, 'onClick') && !attribute(node, 'onPointerDown') && !attribute(node, 'type'))
    expect(inert.map((node) => source.getLineAndCharacterOfPosition(node.getStart()).line + 1)).toEqual([])
  })

  it('creates the duplicate selection identity before React schedules a state updater', () => {
    const duplicate = descendants(source, (node) => ts.isFunctionDeclaration(node) && node.name?.text === 'duplicateLayer')[0] as ts.FunctionDeclaration
    expect(duplicate?.body).toBeDefined()
    const declaration = descendants(duplicate, (node) => ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.name.text === 'duplicateId')[0] as ts.VariableDeclaration
    expect(declaration?.initializer && ts.isCallExpression(declaration.initializer)).toBe(true)
    const mutations = calls(duplicate, 'mutateLayers')
    const selection = calls(duplicate, 'setSelectedLayerId')
    expect(mutations).toHaveLength(1)
    expect(selection).toHaveLength(1)
    expect(declaration.getStart()).toBeLessThan(mutations[0].getStart())
    expect(selection[0].arguments[0] && ts.isIdentifier(selection[0].arguments[0]) && selection[0].arguments[0].text).toBe('duplicateId')
    const assignments = descendants(mutations[0], (node) => ts.isBinaryExpression(node)
      && node.operatorToken.kind === ts.SyntaxKind.EqualsToken
      && ts.isIdentifier(node.left) && node.left.text === 'duplicateId')
    expect(assignments).toHaveLength(0)
    expect(identifier(mutations[0], 'duplicateId')).toBe(true)
  })

  it('guards selection-scoped native completions before they update the editor', () => {
    const handlers = [
      ['selectReference', 'chooseNativeReferencePath', 'setReferencePath'],
      ['analyzeReference', 'matchNativeReference', 'setReferenceResult'],
      ['loadLookWorkflow', 'applyNativeLook', 'applyWorkflowSettings'],
      ['selectMixerLook', 'chooseNativeLookPath', 'setLookAPath'],
      ['applyStyleMixer', 'mixNativeLooks', 'applyWorkflowSettings'],
      ['pickMixerBand', 'sampleNativeColor', 'setMixerBand'],
      ['refreshOpticsStatus', 'resolveNativeOpticsStatus', 'setOpticsStatus'],
      ['detectPortrait', 'detectNativePortrait', 'setPortraitDetection'],
      ['generateAiMask', 'generateNativeAiMask', 'mutateLayers'],
      ['runAdvisor', 'adviseNativeImage', 'setAdvisorResult'],
    ] as const
    for (const [name, provider, mutation] of handlers) {
      const handler = functionBody(name)
      const token = descendants(handler, (node) => ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.name.text === 'current')[0] as ts.VariableDeclaration
      expect(token?.initializer && ts.isCallExpression(token.initializer), `${name} must begin a request token`).toBe(true)
      const begin = token.initializer as ts.CallExpression
      expect(ts.isPropertyAccessExpression(begin.expression) && begin.expression.name.text).toBe('begin')
      expect(identifier(begin.expression, 'editorRequests')).toBe(true)
      const request = calls(handler, provider)[0]
      expect(request, `${name} must call its real native provider`).toBeDefined()
      expect(ts.isAwaitExpression(request.parent), `${name} must await its provider completion`).toBe(true)
      expect(token.getStart()).toBeLessThan(request.getStart())
      const update = calls(handler, mutation).find((call) => call.getStart() > request.getEnd())
      expect(update, `${name} must publish its native result`).toBeDefined()
      const guards = descendants(handler, (node) => ts.isIfStatement(node) && currentGuard(node.expression)) as ts.IfStatement[]
      const guardedUpdate = guards.some((guard) => {
        if (guard.getStart() <= request.getEnd()) return false
        // A positive current() condition enclosing the mutation, or a preceding
        // stale-result early return, are both valid production publication paths.
        if (guard.thenStatement.getStart() <= update!.getStart() && guard.thenStatement.getEnd() >= update!.getEnd()) return true
        return guard.getEnd() < update!.getStart() && descendants(guard.thenStatement, ts.isReturnStatement).length > 0
      })
      expect(guardedUpdate, `${name} must check its request token after await and before publication`).toBe(true)
    }
  })

  it('rejects brush and healing over-capacity without silently slicing existing edits', () => {
    for (const name of ['updateHealingOperations', 'addHealingStroke', 'addMaskBrushStroke']) {
      const handler = functionBody(name)
      const slices = descendants(handler, (node) => ts.isCallExpression(node)
        && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'slice')
      expect(slices, `${name} must not silently truncate edit history or stroke coverage`).toHaveLength(0)
    }
    for (const name of ['addHealingStroke', 'addMaskBrushStroke']) {
      const handler = functionBody(name)
      expect(calls(handler, 'appendWithinCapacity')).toHaveLength(1)
      const rejection = descendants(handler, (node) => ts.isIfStatement(node)
        && identifier(node.expression, 'appended') && identifier(node.expression, 'ok')) as ts.IfStatement[]
      expect(rejection).toHaveLength(1)
      expect(calls(rejection[0].thenStatement, 'setNotice')).toHaveLength(1)
      expect(descendants(rejection[0].thenStatement, ts.isReturnStatement)).toHaveLength(1)
    }
  })

  it('disables manual brush point creation at the same native capacity limit', () => {
    const controls = functionBody('LayerMaskControls')
    const buttons = descendants(controls, (node) => opening(node)?.tagName.getText(source) === 'button')
    const add = buttons.find((node) => {
      const click = attribute(node, 'onClick')?.initializer
      return click && descendants(click, (value) => ts.isPropertyAssignment(value)
        && value.name.getText(source) === 'points' && ts.isArrayLiteralExpression(value.initializer)
        && value.initializer.elements.some(ts.isSpreadElement)).length > 0
    })
    expect(add).toBeDefined()
    const disabled = attribute(add!, 'disabled')?.initializer
    const cap = disabled && descendants(disabled, (node) => ts.isBinaryExpression(node)
      && node.operatorToken.kind === ts.SyntaxKind.GreaterThanEqualsToken
      && ts.isPropertyAccessExpression(node.left) && node.left.name.text === 'length'
      && identifier(node.left, 'points') && ts.isNumericLiteral(node.right) && node.right.text === '8192')
    expect(cap).toHaveLength(1)
  })

  it('serializes snapshot rename/delete and prevents old-asset completion from replacing active history', () => {
    for (const [name, provider] of [['renameSnapshot', 'renameNativeSnapshot'], ['deleteSnapshot', 'deleteNativeSnapshot']] as const) {
      const handler = functionBody(name)
      expect(calls(handler, 'flushNativeHistory')).toHaveLength(1)
      const request = calls(handler, provider)
      expect(request).toHaveLength(1)
      const queue = descendants(handler, (node) => ts.isCallExpression(node)
        && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'run'
        && identifier(node.expression, 'historyCommands')) as ts.CallExpression[]
      expect(queue).toHaveLength(1)
      expect(request[0].getStart()).toBeGreaterThan(queue[0].getStart())
      expect(request[0].getEnd()).toBeLessThan(queue[0].getEnd())
      const update = calls(handler, 'setNativeHistory')
      expect(update).toHaveLength(1)
      const guards = descendants(handler, (node) => ts.isIfStatement(node)
        && identifier(node.expression, 'selectedHistoryAsset') && identifier(node.expression, 'assetId')) as ts.IfStatement[]
      expect(guards).toHaveLength(1)
      const guard = guards[0]
      const enclosed = update[0].getStart() >= guard.thenStatement.getStart() && update[0].getEnd() <= guard.thenStatement.getEnd()
      const staleReturn = guard.getEnd() < update[0].getStart() && descendants(guard.thenStatement, ts.isReturnStatement).length > 0
      expect(enclosed || staleReturn, `${name} must publish history only for the still-selected asset`).toBe(true)
    }
  })

  it('persists native header and Develop keyboard ratings through the single-asset database action', () => {
    const toggle = functionBody('toggleRating')
    const nativeBranch = descendants(toggle, (node) => ts.isIfStatement(node)
      && ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'libraryAsset')[0] as ts.IfStatement
    expect(nativeBranch).toBeDefined()
    expect(calls(nativeBranch.thenStatement, 'rateLibraryAsset')).toHaveLength(1)
    expect(descendants(nativeBranch.thenStatement, ts.isReturnStatement)).toHaveLength(1)
    const dispatch = functionBody('executeCommand')
    const numericDispatch = descendants(dispatch, ts.isDefaultClause)[0]
    const developBranch = descendants(numericDispatch, (node) => ts.isIfStatement(node)
      && identifier(node.expression, 'libraryAsset') && identifier(node.expression, 'view'))[0] as ts.IfStatement
    expect(developBranch).toBeDefined()
    expect(calls(developBranch.thenStatement, 'rateLibraryAsset')).toHaveLength(1)
    expect(calls(numericDispatch, 'updateLibraryWorkflow')).toHaveLength(1)
    const persist = functionBody('rateLibraryAsset')
    const write = calls(persist, 'updateNativeLibraryWorkflow')
    expect(write).toHaveLength(1)
    expect(ts.isArrayLiteralExpression(write[0].arguments[0]) && identifier(write[0].arguments[0], 'assetId')).toBe(true)
    expect(ts.isAwaitExpression(write[0].parent)).toBe(true)
  })

  it('connects Library information disclosure to an actual nonzero inspector track', () => {
    const button = descendants(source, (node) => hasClass(node, 'library-info-toggle'))[0]
    expect(button).toBeDefined()
    expect(identifier(attribute(button, 'aria-expanded')!.initializer!, 'libraryMetadataOpen')).toBe(true)
    expect(calls(attribute(button, 'onClick')!.initializer!, 'setLibraryMetadataOpen')).toHaveLength(1)
    const workspace = descendants(source, (node) => opening(node)?.tagName.getText(source) === 'div'
      && Boolean(attribute(node, 'className')?.initializer && identifier(attribute(node, 'className')!.initializer!, 'libraryMetadataOpen')))[0]
    expect(workspace).toBeDefined()
    expect(identifier(attribute(workspace, 'className')!.initializer!, 'exportPanelOpen')).toBe(true)
    const side = descendants(source, (node) => hasClass(node, 'inspector-panel'))[0]
    const hidden = attribute(side, 'hidden')!.initializer!
    expect(identifier(hidden, 'libraryMetadataOpen')).toBe(true)
    expect(identifier(hidden, 'exportPanelOpen')).toBe(true)
    expect(winningValue('.workspace.library-inspector-open', '--library-inspector-width')).toBe('300px')
    for (const width of [920, 1040, 1180, 1279, 1920]) {
      for (const selector of ['.theme-dark .workspace.view-library', '.theme-dark .workspace.view-library.left-collapsed']) {
        expect(winningValue(selector, 'grid-template-columns', width), `${selector} at ${width}px must expose the requested information panel`).toContain('var(--library-inspector-width)')
      }
    }
    expect(winningValue('.inspector-panel[hidden]', 'display')).toBe('none')
  })

  it('makes the non-Native demo read-only at both interaction and edit-state boundaries', () => {
    const groups = descendants(source, (node) => hasClass(node, 'native-edit-controls'))
    expect(groups).toHaveLength(2)
    for (const group of groups) {
      expect(opening(group)?.tagName.getText(source)).toBe('fieldset')
      for (const name of ['disabled', 'inert']) {
        const condition = attribute(group, name)?.initializer
        expect(condition && identifier(condition, 'renderBackend')).toBe(true)
        expect(condition && descendants(condition, (node) => ts.isBinaryExpression(node)
          && node.operatorToken.kind === ts.SyntaxKind.ExclamationEqualsEqualsToken
          && ts.isStringLiteral(node.right) && node.right.text === 'native')).toHaveLength(1)
      }
    }
    const edit = functionBody('updateSelected')
    const guard = edit.body?.statements[0]
    expect(guard && ts.isIfStatement(guard)).toBe(true)
    const rejection = guard as ts.IfStatement
    expect(identifier(rejection.expression, 'renderBackend')).toBe(true)
    expect(descendants(rejection.thenStatement, ts.isReturnStatement)).toHaveLength(1)
    expect(calls(rejection.thenStatement, 'setNotice')).toHaveLength(1)
    expect(calls(rejection.thenStatement, 'setPhotos')).toHaveLength(0)
  })

  it('routes both header and palette export through editable production export settings', () => {
    const header = descendants(source, (node) => opening(node)?.tagName.getText(source) === 'AppHeader')[0]
    const click = attribute(header, 'onExport')!.initializer!
    expect(calls(click, 'setExportPanelOpen')).toHaveLength(1)
    expect(calls(click, 'exportJpeg')).toHaveLength(0)
    const dispatch = functionBody('executeCommand')
    const exportCase = descendants(dispatch, (node) => ts.isCaseClause(node) && ts.isStringLiteral(node.expression) && node.expression.text === 'export')[0]
    expect(calls(exportCase, 'setExportPanelOpen')).toHaveLength(1)
    expect(calls(exportCase, 'exportJpeg')).toHaveLength(0)
    const panel = descendants(source, (node) => opening(node)?.tagName.getText(source) === 'ExportPanel')[0]
    expect(calls(attribute(panel, 'onExport')!.initializer!, 'exportJpeg')).toHaveLength(1)
    const hiddenChildren = rulesFor('.view-library .inspector-panel > :not(.library-metadata):not(.export-popover)')
    expect(hiddenChildren).toHaveLength(1)
    expect(rulesFor('.view-library .inspector-panel > :not(.library-metadata)')).toHaveLength(0)
  })

  it('keeps compact workspace navigation visible and does not falsely label every native preview as CPU', () => {
    for (const width of [920, 1040, 1180, 1279, 1920]) {
      expect(winningValue('.topbar nav', 'display', width)).toBe('flex')
      expect(winningValue('.theme-dark .topbar nav', 'display', width)).toBe('flex')
    }
    const badge = descendants(source, (node) => hasClass(node, 'preview-badge'))[0]
    expect(badge).toBeDefined()
    const fixedCpuLabels = descendants(badge, (node) => ts.isStringLiteral(node) && node.text === 'Native CPU')
    expect(fixedCpuLabels).toHaveLength(0)
    expect(identifier(badge, 'renderBackend')).toBe(true)
  })

  it('provides distinct RGB histogram colors in all themes, not only dark glass', () => {
    const fills: string[] = [], strokes: string[] = []
    for (const channel of ['red', 'green', 'blue']) {
      // Unscoped production rules apply to gray/light as well as dark; the
      // glass theme may layer brighter colors over this shared base.
      const selector = `.histogram-channel.${channel}`
      const fill = winningValue(selector, 'fill'), stroke = winningValue(selector, 'stroke')
      expect(fill, `${channel} must have a shared fill outside .theme-dark`).toBeDefined()
      expect(stroke, `${channel} must have a shared stroke outside .theme-dark`).toBeDefined()
      expect(rulesFor(selector).every((rule) => rule.parent?.type === 'root')).toBe(true)
      fills.push(fill!); strokes.push(stroke!)
      expect(winningValue(`.theme-dark ${selector}`, 'fill')).toBeDefined()
    }
    expect(new Set(fills).size).toBe(3)
    expect(new Set(strokes).size).toBe(3)
    expect(winningValue('.histogram-channel.luminance', 'fill')).toBeDefined()
  })
})
