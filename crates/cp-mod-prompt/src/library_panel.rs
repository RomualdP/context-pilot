use crossterm::event::KeyEvent;

use crate::types::{PromptState, PromptType};

use cp_base::panels::{CacheRequest, CacheUpdate, ContextItem, Panel, scroll_key_action};
use cp_base::state::actions::Action;
use cp_base::state::context::{Entry, Kind};
use cp_base::state::runtime::State;
use std::fmt::Write as _;

/// Panel displaying the full prompt library (agents, skills, commands).
pub(crate) struct LibraryPanel;

impl Panel for LibraryPanel {
    fn handle_key(&self, key: &KeyEvent, _state: &State) -> Option<Action> {
        scroll_key_action(key)
    }

    fn needs_cache(&self) -> bool {
        false
    }

    fn refresh_cache(&self, _request: CacheRequest) -> Option<CacheUpdate> {
        None
    }

    fn build_cache_request(&self, _ctx: &Entry, _state: &State) -> Option<CacheRequest> {
        None
    }

    fn apply_cache_update(&self, _update: CacheUpdate, _ctx: &mut Entry, _state: &mut State) -> bool {
        false
    }

    fn cache_refresh_interval_ms(&self) -> Option<u64> {
        None
    }

    fn suicide(&self, _ctx: &Entry, _state: &State) -> bool {
        false
    }

    fn blocks(&self, state: &State) -> Vec<cp_render::Block> {
        crate::library_blocks::library_blocks(state)
    }

    fn title(&self, _state: &State) -> String {
        "Library".to_string()
    }

    fn refresh(&self, state: &mut State) {
        let items = self.context(state);
        if let Some(ctx) = state.context.iter_mut().find(|c| c.context_type == Kind::new(Kind::LIBRARY)) {
            let total: usize = items.iter().map(|i| cp_base::state::context::estimate_tokens(&i.content)).sum();
            ctx.token_count = total;
            let combined: String = items.iter().map(|i| i.content.as_str()).collect::<Vec<_>>().join("\n");
            let _ = cp_base::panels::update_if_changed(ctx, &combined);
        }
    }

    fn max_freezes(&self) -> u8 {
        3
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        let Some(ctx) = state.context.iter().find(|c| c.context_type == Kind::new(Kind::LIBRARY)) else {
            return Vec::new();
        };

        let ps = PromptState::get(state);
        let agents = crate::storage::load_prompts_for(PromptType::Agent);
        let skills = crate::storage::load_prompts_for(PromptType::Skill);
        let commands = crate::storage::load_prompts_for(PromptType::Command);

        let mut content = String::new();

        // Agents table
        content.push_str("Agents (system prompts):\n\n");
        content.push_str("| ID | Name | Active | Description |\n");
        content.push_str("|------|------|--------|-------------|\n");
        for agent in &agents {
            let active = if ps.active_agent_id.as_deref() == Some(&agent.id) { "✓" } else { "" };
            let _wa = writeln!(content, "| {} | {} | {} | {} |", agent.id, agent.name, active, agent.description);
        }

        // Skills table
        if !skills.is_empty() {
            content.push_str("\nSkills (use skill_load to load, Close_panel to unload):\n\n");
            content.push_str("| ID | Name | Loaded | Description |\n");
            content.push_str("|------|------|--------|-------------|\n");
            for skill in &skills {
                let loaded = if ps.loaded_skill_ids.contains(&skill.id) { "✓" } else { "" };
                let _wb = writeln!(content, "| {} | {} | {} | {} |", skill.id, skill.name, loaded, skill.description);
            }
        }

        // Commands table
        if !commands.is_empty() {
            content.push_str("\nCommands:\n\n");
            content.push_str("| Command | Name | Description |\n");
            content.push_str("|---------|------|-------------|\n");
            for cmd in &commands {
                let _wc = writeln!(content, "| /{} | {} | {} |", cmd.id, cmd.name, cmd.description);
            }
        }

        // File paths info (so the AI knows where to edit/delete)
        content.push_str("\nFile locations:\n");
        let _wd = writeln!(content, "- Agents: {}/", crate::storage::dir_for(PromptType::Agent).display());
        let _we = writeln!(content, "- Skills: {}/", crate::storage::dir_for(PromptType::Skill).display());
        let _wf = writeln!(content, "- Commands: {}/", crate::storage::dir_for(PromptType::Command).display());
        content.push_str("\nTo edit: use the Edit tool on the .md file directly.");
        content.push_str("\nTo delete: delete the .md file.\n");

        vec![ContextItem::new(&ctx.id, "Library", content, ctx.last_refresh_ms)]
    }
}
