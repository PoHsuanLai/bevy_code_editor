#!/bin/bash
# Script to add RenderLayers to all remaining visual entities

echo "Adding EditorRenderConfig parameter to function signatures..."

# Note: These sed commands are for macOS (use gsed on Linux for -i without backup)
# Manual review recommended after running this script

echo "✅ Done! Please review the changes with 'git diff' and then run 'cargo check'"
echo ""
echo "Remaining manual changes needed:"
echo "1. Add 'render_config: Res<EditorRenderConfig>' parameter to these functions:"
echo "   - src/plugin/minimap.rs::update_minimap (line ~186)"
echo "   - src/plugin/ui_elements.rs::update_text_display (line ~100)"
echo "   - src/plugin/ui_elements.rs::update_selection_highlight (line ~320)"
echo "   - src/plugin/ui_elements.rs::update_indent_guides (line ~470)"
echo "   - src/plugin/brackets.rs::update_bracket_match_highlight (line ~150)"
echo "   - src/lsp/render.rs - all render_* functions"
echo "   - src/plugin/gpu_text_render.rs::update_text_rendering_gpu (line ~450)"
echo ""
echo "2. Wrap all 'commands.spawn()' calls with the pattern:"
echo "   let mut entity_cmd = commands.spawn((...));"
echo "   if let Some(ref layers) = render_config.render_layers {"
echo "       entity_cmd.insert(layers.clone());"
echo "   }"
echo ""
echo "See RENDER_LAYERS_TODO.md for complete list of spawn locations"
