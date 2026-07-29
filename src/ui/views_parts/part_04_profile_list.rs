impl SanAndreasModUi
{
    pub(super) fn profile_mod_order_list(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[ProfileModEntry],
        visible: Option<&std::collections::BTreeSet<String>>,
    )
    {
        let count = entries.len();
        let order_ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        // Per-mod content flags come from the last content scan (if any). Cheap
        // to derive from the cached index; absent until the user analyzes.
        let flags_map = self.content.index.as_ref().map(per_mod_flags);
        let overwrite_color = egui::Color32::from_rgb(
            RUNNING_STATUS_RED,
            RUNNING_STATUS_GREEN,
            RUNNING_STATUS_BLUE,
        );
        let overwritten_color = ui.visuals().warn_fg_color;
        // Subsystem badges (ModLoader / CLEO / ASI) derived from each mod's
        // install-root kinds â€” always shown, no content scan required.
        let subsystems: std::collections::BTreeMap<String, (bool, bool, bool)> = self
            .state
            .mods
            .iter()
            .map(|item| {
                let mut modloader = false;
                let mut cleo = false;
                let mut asi = false;
                for root in &item.config.install_roots
                {
                    match root.kind.to_ascii_lowercase().as_str()
                    {
                        "modloader" => modloader = true,
                        "cleo" | "cleo_text" | "cleo_plugin" => cleo = true,
                        "asi" | "plugin" => asi = true,
                        _ => {}
                    }
                }
                (item.config.id.clone(), (modloader, cleo, asi))
            })
            .collect();
        // Per-mod color label + note presence, snapshotted so rows read them
        // without holding a borrow on `self.mod_meta`.
        let mod_marks: std::collections::BTreeMap<String, (Option<egui::Color32>, bool)> = self
            .mod_meta
            .iter()
            .map(|(id, meta)| {
                (
                    id.clone(),
                    (
                        meta.color.as_deref().and_then(meta_color),
                        !meta.note.trim().is_empty(),
                    ),
                )
            })
            .collect();
        // A filter hides rows, which would make drag targets and priority indices
        // point at the wrong slots, so ordering is disabled while one is active.
        let reorderable = visible.is_none();
        // Deferred side effects (at most one fires per frame in practice).
        let mut activation: Option<(String, bool)> = None;
        let mut remove: Option<String> = None;
        let mut reorder: Option<Vec<String>> = None;
        // (mod id, conflicts_only) for a "Show in Content" cross-link click.
        let mut focus: Option<(String, bool)> = None;
        // Mod id to open the per-mod info window for.
        let mut details: Option<String> = None;
        // Separators (labeled dividers) render only in the unfiltered, reorderable
        // view â€” they are organizational and reordering is off while filtering.
        let entries_len = entries.len();
        let separators = if reorderable {
            self.separators.clone()
        } else {
            Vec::new()
        };
        let collapsed = self.collapsed_separators.clone();
        let mut sep_edit = self.separator_edit.clone();
        let hidden = collapsed_hidden_indices(&separators, &collapsed, entries_len);
        // Deferred separator effects.
        let mut sep_remove: Option<String> = None;
        let mut sep_move: Option<(String, usize)> = None;
        let mut sep_collapse: Option<String> = None;
        let mut sep_rename: Option<(String, String)> = None;
        let mut sep_start_edit: Option<String> = None;
        // Fixed column widths so every row lines up as a real table; the name
        // column flexes to fill whatever the centre pane leaves.
        const GRIP_W: f32 = 18.0;
        const ON_W: f32 = 26.0;
        const PRIO_W: f32 = 52.0;
        const MOVE_W: f32 = 54.0;
        const SUBSYS_W: f32 = 78.0;
        const FLAGS_W: f32 = 170.0;
        const MAX_CHIPS: usize = 4;
        // Reserve for the trailing Remove/â‹¯ actions, inter-cell spacing, and the
        // scrollbar; underfill (a small gap) rather than overflow the row.
        let name_w = (ui.available_width()
            - GRIP_W
            - ON_W
            - PRIO_W
            - MOVE_W
            - SUBSYS_W
            - FLAGS_W
            - MOD_ROW_NAME_RESERVED_WIDTH)
            .max(MOD_ROW_NAME_MIN_WIDTH);
        ui.horizontal(|ui| {
            table_cell(ui, GRIP_W, |_ui| {});
            table_cell(ui, ON_W, |ui| {
                ui.strong("On");
            });
            table_cell(ui, PRIO_W, |ui| {
                ui.strong("#").on_hover_text("Load-order priority");
            });
            table_cell(ui, MOVE_W, |_ui| {});
            table_cell(ui, name_w, |ui| {
                ui.strong("Mod");
            });
            table_cell(ui, SUBSYS_W, |ui| {
                ui.strong("Subsys").on_hover_text("ModLoader / CLEO / ASI");
            });
            table_cell(ui, FLAGS_W, |ui| {
                ui.strong("Flags");
            });
            ui.strong("Actions");
        });
        ui.separator();
        if !reorderable
        {
            ui.weak("Reordering is disabled while a filter is active â€” clear filters to drag or renumber.");
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Reorder resolved from a completed drag (source â†’ insertion slot).
                let mut drag_from: Option<usize> = None;
                let mut drop_to: Option<usize> = None;
                for (idx, entry) in entries.iter().enumerate()
                {
                    // Any separators anchored to this display slot render above it.
                    for sep in separators.iter().filter(|sep| sep.position == idx)
                    {
                        let editing = sep_edit.as_ref().is_some_and(|(id, _)| *id == sep.id);
                        let mut buf = sep_edit
                            .as_ref()
                            .map(|(_, name)| name.clone())
                            .unwrap_or_default();
                        let action = separator_header(
                            ui,
                            sep,
                            SeparatorHeaderState {
                                collapsed: collapsed.contains(&sep.id),
                                editing,
                                buf: &mut buf,
                            },
                        );
                        let edit_state = SeparatorEditState::from_editing(editing);
                        update_separator_edit_buffer(edit_state, sep_edit.as_mut(), &buf);
                        match action
                        {
                            Some(SepAction::ToggleCollapse) => sep_collapse = Some(sep.id.clone()),
                            Some(SepAction::StartEdit) => sep_start_edit = Some(sep.id.clone()),
                            Some(SepAction::Commit) => sep_rename = Some((sep.id.clone(), buf)),
                            Some(SepAction::Remove) => sep_remove = Some(sep.id.clone()),
                            Some(SepAction::MoveUp) => {
                                sep_move = Some((sep.id.clone(), sep.position.saturating_sub(1)));
                            }
                            Some(SepAction::MoveDown) => {
                                sep_move =
                                    Some((sep.id.clone(), (sep.position + 1).min(entries_len)));
                            }
                            None => {}
                        }
                    }
                    // Skip rows hidden by the active filter, but keep `idx` tied to
                    // the full list so priority ranks stay truthful.
                    if let Some(set) = visible
                    {
                        if !set.contains(&entry.id)
                        {
                            continue;
                        }
                    }
                    // Hidden inside a collapsed separator section.
                    if hidden.contains(&idx)
                    {
                        continue;
                    }
                    // A move requested by this row via a button or the index field.
                    let mut move_to: Option<usize> = None;
                    let row = ui
                        .horizontal(|ui| {
                            // Drag handle â€” the drag source carries this row index.
                            table_cell(ui, GRIP_W, |ui| {
                                if reorderable
                                {
                                    ui.dnd_drag_source(
                                        egui::Id::new(("mod_grip", &entry.id)),
                                        idx,
                                        |ui| {
                                            ui.label("â£¿").on_hover_text("Drag to reorder");
                                        },
                                    );
                                }
                                else
                                {
                                    ui.weak("â£¿");
                                }
                            });
                            table_cell(ui, ON_W, |ui| {
                                let mut enabled = entry.enabled;
                                if ui
                                    .checkbox(&mut enabled, "")
                                    .on_hover_text(
                                        "Enable or disable this mod in the current profile",
                                    )
                                    .changed()
                                {
                                    activation = Some((entry.id.clone(), enabled));
                                }
                            });
                            table_cell(ui, PRIO_W, |ui| {
                                if reorderable
                                {
                                    // 1-based priority; typing a new number moves it.
                                    let mut position = idx + 1;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut position)
                                                .range(1..=count.max(1))
                                                .speed(MOD_ROW_DRAG_SPEED),
                                        )
                                        .on_hover_text(
                                            "Priority â€” type a number to move this mod there",
                                        )
                                        .changed()
                                    {
                                        let target = position.clamp(1, count) - 1;
                                        if target != idx
                                        {
                                            move_to = Some(target);
                                        }
                                    }
                                }
                                else
                                {
                                    // Read-only true load-order rank while filtered.
                                    ui.monospace(format!("{:>3}", idx + 1)).on_hover_text(
                                        "Load-order position â€” clear filters to change",
                                    );
                                }
                            });
                            table_cell(ui, MOVE_W, |ui| {
                                if reorderable
                                {
                                    if ui
                                        .add_enabled(idx > 0, egui::Button::new("â–²").small())
                                        .on_hover_text("Move up (loads earlier)")
                                        .clicked()
                                    {
                                        move_to = Some(idx - 1);
                                    }
                                    if ui
                                        .add_enabled(
                                            idx + 1 < count,
                                            egui::Button::new("â–¼").small(),
                                        )
                                        .on_hover_text("Move down (loads later, overwrites)")
                                        .clicked()
                                    {
                                        move_to = Some(idx + 1);
                                    }
                                }
                            });
                            // The mod name is itself a drag source (when ordering
                            // is allowed), so the row body â€” not just the â£¿ handle
                            // â€” can be grabbed to reorder. Truncated to the column
                            // and greyed when disabled.
                            table_cell(ui, name_w, |ui| {
                                let marks =
                                    mod_marks.get(&entry.id).copied().unwrap_or((None, false));
                                if let Some(color) = marks.0
                                {
                                    ui.colored_label(color, "â—").on_hover_text("Color label");
                                }
                                let text = if entry.enabled {
                                    egui::RichText::new(&entry.id)
                                } else {
                                    egui::RichText::new(&entry.id).weak()
                                };
                                // Double-click opens the Mod Info dialog. When the
                                // list is reorderable the name is also a drag
                                // source (click_and_drag), so dragging it still
                                // reorders â€” we feed the drag payload manually.
                                let sense = if reorderable {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::click()
                                };
                                let name = ui.add(egui::Label::new(text).truncate().sense(sense));
                                if reorderable && name.dragged()
                                {
                                    egui::DragAndDrop::set_payload(ui.ctx(), idx);
                                }
                                if name.double_clicked()
                                {
                                    details = Some(entry.id.clone());
                                }
                                name.on_hover_text(entry.config.display().to_string());
                                if marks.1
                                {
                                    ui.small("ðŸ“").on_hover_text("Has a note (see Details)");
                                }
                            });
                            // Subsystem badges (always on): which loader systems
                            // this mod installs into.
                            table_cell(ui, SUBSYS_W, |ui| {
                                if let Some((modloader, cleo, asi)) =
                                    subsystems.get(&entry.id).copied()
                                {
                                    if modloader
                                    {
                                        let modloader_color = egui::Color32::from_rgb(
                                            MODLOADER_BADGE_RED,
                                            MODLOADER_BADGE_GREEN,
                                            MODLOADER_BADGE_BLUE,
                                        );
                                        ui.colored_label(modloader_color, "M")
                                            .on_hover_text("ModLoader mod");
                                    }
                                    if cleo
                                    {
                                        let cleo_color = egui::Color32::from_rgb(
                                            RUNNING_STATUS_RED,
                                            RUNNING_STATUS_GREEN,
                                            RUNNING_STATUS_BLUE,
                                        );
                                        ui.colored_label(cleo_color, "C")
                                            .on_hover_text("CLEO script");
                                    }
                                    if asi
                                    {
                                        let asi_color = egui::Color32::from_rgb(
                                            ASI_BADGE_RED,
                                            ASI_BADGE_GREEN,
                                            ASI_BADGE_BLUE,
                                        );
                                        ui.colored_label(asi_color, "A")
                                            .on_hover_text("ASI plugin");
                                    }
                                }
                            });
                            // Content flags (MO2-style): conflict arrows first,
                            // then compact category chips (capped). Only present
                            // once the profile's content has been analyzed.
                            table_cell(ui, FLAGS_W, |ui| {
                                if let Some(flags) =
                                    flags_map.as_ref().and_then(|map| map.get(&entry.id))
                                {
                                    if flags.overwrites_others
                                    {
                                        ui.colored_label(overwrite_color, "â¬†").on_hover_text(
                                            "Overwrites files from lower-priority mods",
                                        );
                                    }
                                    if flags.overwritten
                                    {
                                        ui.colored_label(overwritten_color, "â¬‡").on_hover_text(
                                            "Some files are overwritten by higher-priority mods",
                                        );
                                    }
                                    for category in flags.categories.iter().take(MAX_CHIPS)
                                    {
                                        ui.small(category.short_label())
                                            .on_hover_text(category.label());
                                    }
                                    let extra = flags.categories.len().saturating_sub(MAX_CHIPS);
                                    if extra > 0
                                    {
                                        ui.small(format!("+{extra}"));
                                    }
                                }
                            });
                            if ui
                                .button("Remove")
                                .on_hover_text("Remove this mod from the profile (asks first)")
                                .clicked()
                            {
                                remove = Some(entry.id.clone());
                            }
                            ui.menu_button("â‹¯", |ui| {
                                if ui.button("Detailsâ€¦").clicked()
                                {
                                    details = Some(entry.id.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Show files in Content").clicked()
                                {
                                    focus = Some((entry.id.clone(), false));
                                    ui.close_menu();
                                }
                                if ui.button("Show conflicts in Content").clicked()
                                {
                                    focus = Some((entry.id.clone(), true));
                                    ui.close_menu();
                                }
                            })
                            .response
                            .on_hover_text("Open this mod's details, files, or conflicts");
                        })
                        .response;
                    if let Some(target) = move_to
                    {
                        reorder = Some(move_in_list(&order_ids, idx, target));
                    }
                    // Drag feedback + drop resolution over the whole row rect â€”
                    // only when ordering is allowed (no filter active).
                    if reorderable
                    {
                        let row_zone = ui.interact(
                            row.rect,
                            egui::Id::new(("mod_row", &entry.id)),
                            egui::Sense::hover(),
                        );
                        draw_mod_row_drop_hover(ui, &row, &row_zone);
                        if let Some((from, to)) = mod_row_drop_release(ui, &row, &row_zone, idx)
                        {
                            drag_from = Some(from);
                            drop_to = Some(to);
                        }
                    }
                }
                // Separators positioned at or past the end sit below every mod.
                for sep in separators.iter().filter(|sep| sep.position >= entries_len)
                {
                    let editing = sep_edit.as_ref().is_some_and(|(id, _)| *id == sep.id);
                    let mut buf = sep_edit
                        .as_ref()
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default();
                    let action = separator_header(
                        ui,
                        sep,
                        SeparatorHeaderState {
                            collapsed: collapsed.contains(&sep.id),
                            editing,
                            buf: &mut buf,
                        },
                    );
                    let edit_state = SeparatorEditState::from_editing(editing);
                        update_separator_edit_buffer(edit_state, sep_edit.as_mut(), &buf);
                    match action
                    {
                        Some(SepAction::ToggleCollapse) => sep_collapse = Some(sep.id.clone()),
                        Some(SepAction::StartEdit) => sep_start_edit = Some(sep.id.clone()),
                        Some(SepAction::Commit) => sep_rename = Some((sep.id.clone(), buf)),
                        Some(SepAction::Remove) => sep_remove = Some(sep.id.clone()),
                        Some(SepAction::MoveUp) => {
                            sep_move = Some((sep.id.clone(), sep.position.saturating_sub(1)));
                        }
                        Some(SepAction::MoveDown) => {
                            sep_move = Some((sep.id.clone(), (sep.position + 1).min(entries_len)));
                        }
                        None => {}
                    }
                }
                if let (Some(from), Some(to)) = (drag_from, drop_to)
                {
                    // A drop onto the source's own slot (or its lower edge) is a no-op.
                    if to != from && to != from + 1
                    {
                        let adjusted = if from < to { to - 1 } else { to };
                        reorder = Some(move_in_list(&order_ids, from, adjusted));
                    }
                }
            });
        let actions = ProfileOrderListActions
        {
            activation,
            reorder,
            remove,
            focus,
            details,
            separator_edit: sep_edit,
            separator_start_edit: sep_start_edit,
            separator_collapse: sep_collapse,
            separator_rename: sep_rename,
            separator_move: sep_move,
            separator_remove: sep_remove,
        };
        self.apply_profile_order_list_actions(actions);
    }
}
