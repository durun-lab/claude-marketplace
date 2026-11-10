//! UI のコマンドの表（`commands::TABLE`）。1 行の形（`Spec`）と、押せるかと実行は `commands.rs` が持つ。

use mm_ui_layout::TimelineCommand;

use super::{Action, LayerCommand, LayoutCommand, Owner, ShyCommand, Spec, WindowAction};
use crate::win::edit_actions::EditAction;
use crate::win::playback::Command;

/// コマンドの表。ID の名前の順に並べ、足すときもその位置に入れる。
pub static TABLE: &[Spec] = &[
    Spec {
        id: "app.exit",
        name: "Exit",
        description: None,
        owner: Owner::App(Action::Window(WindowAction::Exit)),
    },
    Spec {
        id: "app.show_about",
        name: "About",
        description: None,
        owner: Owner::App(Action::Window(WindowAction::About)),
    },
    Spec {
        id: "app.show_keyboard_shortcuts",
        name: "Keyboard Shortcuts…",
        description: Some("List every command with its key, and run one by name"),
        owner: Owner::App(Action::Window(WindowAction::ShowKeyboardShortcuts)),
    },
    Spec {
        id: "clip.add_transition",
        name: "Add Transition…",
        description: None,
        owner: Owner::Panel("transition"),
    },
    Spec {
        id: "clip.extract",
        name: "Extract",
        description: Some("Remove the selected clip and close the gap"),
        owner: Owner::App(Action::Edit(EditAction::Extract)),
    },
    Spec {
        id: "clip.insert",
        name: "Insert",
        description: Some("Insert the Source range at the edit point nearest the playhead"),
        owner: Owner::App(Action::Edit(EditAction::Insert)),
    },
    Spec {
        id: "clip.lift",
        name: "Lift",
        description: Some("Remove the selected clip and leave a gap"),
        owner: Owner::App(Action::Edit(EditAction::Lift)),
    },
    Spec {
        id: "clip.overwrite",
        name: "Overwrite",
        description: Some(
            "Replace the selected clip, or the one at the playhead, with the Source range",
        ),
        owner: Owner::App(Action::Edit(EditAction::Overwrite)),
    },
    Spec {
        id: "clip.split",
        name: "Split Clip",
        description: Some("Split the clip at the playhead (Blade Tool splits at the pointer)"),
        owner: Owner::App(Action::Edit(EditAction::Split)),
    },
    Spec {
        id: "clip.split_here",
        name: "Split Clip Here",
        description: Some("Split the clip at the position you right-clicked"),
        owner: Owner::App(Action::Edit(EditAction::SplitHere)),
    },
    Spec {
        id: "edit.redo",
        name: "Redo",
        description: None,
        owner: Owner::App(Action::Edit(EditAction::Redo)),
    },
    Spec {
        id: "edit.undo",
        name: "Undo",
        description: None,
        owner: Owner::App(Action::Edit(EditAction::Undo)),
    },
    Spec {
        id: "export.choose_target",
        name: "Choose Render Target…",
        description: None,
        owner: Owner::Panel("deliver"),
    },
    Spec {
        id: "export.start",
        name: "Export…",
        description: None,
        owner: Owner::Panel("deliver"),
    },
    Spec {
        id: "layer.bring_forward",
        name: "Bring Forward",
        description: Some("Move the selected layer one step toward the front"),
        owner: Owner::App(Action::Layer(LayerCommand::BringForward)),
    },
    Spec {
        id: "layer.send_backward",
        name: "Send Backward",
        description: Some("Move the selected layer one step toward the back"),
        owner: Owner::App(Action::Layer(LayerCommand::SendBackward)),
    },
    Spec {
        id: "layer.toggle_enabled",
        name: "Enable Layer",
        description: Some("Turn the selected layer on or off in the preview and export"),
        owner: Owner::App(Action::Layer(LayerCommand::ToggleEnabled)),
    },
    Spec {
        id: "layer.toggle_lock",
        name: "Lock Layer",
        description: Some("Protect the selected layer from edits made through Claude"),
        owner: Owner::App(Action::Layer(LayerCommand::ToggleLock)),
    },
    Spec {
        id: "media.import",
        name: "Import Media…",
        description: None,
        owner: Owner::Panel("media"),
    },
    Spec {
        id: "playback.go_to_end",
        name: "Go to End",
        description: None,
        owner: Owner::App(Action::Playback(Command::Seek(f64::MAX))),
    },
    Spec {
        id: "playback.go_to_next_keyframe",
        name: "Go to Next Keyframe",
        description: Some(
            "Move the playhead to the next keyframe of the selected layer, or of any layer",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::NextKeyframe)),
    },
    Spec {
        id: "playback.go_to_previous_keyframe",
        name: "Go to Previous Keyframe",
        description: Some(
            "Move the playhead to the previous keyframe of the selected layer, or of any layer",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::PreviousKeyframe)),
    },
    Spec {
        id: "playback.go_to_start",
        name: "Go to Start",
        description: None,
        owner: Owner::App(Action::Playback(Command::Seek(0.0))),
    },
    Spec {
        id: "playback.next_edit",
        name: "Next Edit",
        description: Some("Move the playhead to the next clip or layer boundary"),
        owner: Owner::App(Action::Timeline(TimelineCommand::NextEdit)),
    },
    Spec {
        id: "playback.play_forward",
        name: "Play Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::Play)),
    },
    Spec {
        id: "playback.play_reverse",
        name: "Play Reverse",
        description: Some("Play the video backward without sound"),
        owner: Owner::App(Action::Playback(Command::Reverse)),
    },
    Spec {
        id: "playback.previous_edit",
        name: "Previous Edit",
        description: Some("Move the playhead to the previous clip or layer boundary"),
        owner: Owner::App(Action::Timeline(TimelineCommand::PreviousEdit)),
    },
    Spec {
        id: "playback.step_back",
        name: "Step One Frame Back",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(-1))),
    },
    Spec {
        id: "playback.step_back_second",
        name: "Step One Second Back",
        description: None,
        owner: Owner::App(Action::Playback(Command::StepSeconds(-1))),
    },
    Spec {
        id: "playback.step_forward",
        name: "Step One Frame Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(1))),
    },
    Spec {
        id: "playback.step_forward_second",
        name: "Step One Second Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::StepSeconds(1))),
    },
    Spec {
        id: "playback.stop",
        name: "Stop",
        description: None,
        owner: Owner::App(Action::Playback(Command::Pause)),
    },
    Spec {
        id: "playback.toggle",
        name: "Play / Stop",
        description: None,
        owner: Owner::App(Action::Playback(Command::Toggle)),
    },
    Spec {
        id: "source.clear_in_out",
        name: "Clear In and Out",
        description: Some("Remove the In and Out points of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::ClearInOut)),
    },
    Spec {
        id: "source.mark_in",
        name: "Mark In",
        description: Some("Mark the In point of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::MarkIn)),
    },
    Spec {
        id: "source.mark_out",
        name: "Mark Out",
        description: Some("Mark the Out point of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::MarkOut)),
    },
    Spec {
        id: "timeline.blade_tool",
        name: "Blade Tool",
        description: Some("Click a clip to split it at the pointer"),
        owner: Owner::App(Action::Timeline(TimelineCommand::BladeTool)),
    },
    Spec {
        id: "timeline.full_extent_zoom",
        name: "Full Extent Zoom",
        description: Some(
            "Fit the whole composition in the Timeline, or return to the previous zoom",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::FullExtentZoom)),
    },
    Spec {
        id: "timeline.hide_shy_layers",
        name: "Hide Shy Layers",
        description: Some(
            "Hide the Shy layers from the Timeline; rendering and export don't change",
        ),
        owner: Owner::App(Action::Shy(ShyCommand::HideShyLayers)),
    },
    Spec {
        id: "timeline.remove_keyframe",
        name: "Remove Keyframe",
        description: Some("Remove the keyframe selected in the Timeline's keyframe rows"),
        owner: Owner::App(Action::Timeline(TimelineCommand::RemoveKeyframe)),
    },
    Spec {
        id: "timeline.selection_tool",
        name: "Selection Tool",
        description: Some("Drag a layer to move it in time; select clips and trim their edges"),
        owner: Owner::App(Action::Timeline(TimelineCommand::SelectionTool)),
    },
    Spec {
        id: "timeline.show_keyframe_editor",
        name: "Show Keyframe Editor",
        description: Some(
            "Open or close a row per keyframed property below the selected layer, or all layers",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::ShowKeyframeEditor)),
    },
    Spec {
        id: "timeline.shy_selected_layers",
        name: "Shy Selected Layers",
        description: Some("Mark the selected layer Shy so Hide Shy Layers can hide its row"),
        owner: Owner::App(Action::Shy(ShyCommand::ShySelectedLayers)),
    },
    Spec {
        id: "timeline.toggle_snapping",
        name: "Snapping",
        description: Some(
            "Snap dragged edges to the playhead, clip and layer edges, keyframes, and markers",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping)),
    },
    Spec {
        id: "timeline.trim_tool",
        name: "Trim Tool",
        description: Some("Drag inside a clip to slip its content; Shift+drag to slide the clip"),
        owner: Owner::App(Action::Timeline(TimelineCommand::TrimTool)),
    },
    Spec {
        id: "timeline.zoom_in",
        name: "Zoom In",
        description: Some("Zoom in around the playhead"),
        owner: Owner::App(Action::Timeline(TimelineCommand::ZoomIn)),
    },
    Spec {
        id: "timeline.zoom_out",
        name: "Zoom Out",
        description: Some("Zoom out around the playhead"),
        owner: Owner::App(Action::Timeline(TimelineCommand::ZoomOut)),
    },
    Spec {
        id: "view.reset_ui_layout",
        name: "Reset UI Layout",
        description: Some("Put the splitters and both panel columns back to the default layout"),
        owner: Owner::App(Action::Layout(LayoutCommand::ResetUiLayout)),
    },
    show_panel!("deliver", "Deliver", None),
    show_panel!("devices", "Devices", None),
    show_panel!(
        "diagnostics",
        "Diagnostics",
        Some("Show the GPU, clock, sync lag, backends, and the raw layer list")
    ),
    show_panel!("feedback", "Feedback", None),
    show_panel!("inspector", "Inspector", None),
    show_panel!("media", "Media", None),
    show_panel!("mixer", "Mixer", None),
    show_panel!("review", "Review", None),
    show_panel!("story", "Story", None),
    show_panel!("tasks", "Tasks", None),
    show_panel!("transition", "Transitions", None),
    Spec {
        id: "view.toggle_left_column",
        name: "Left Panels",
        description: Some("Show or hide the column with Tasks, Media, Story, and the other panels"),
        owner: Owner::App(Action::Layout(LayoutCommand::ToggleLeftColumn)),
    },
    Spec {
        id: "view.toggle_right_column",
        name: "Right Panels",
        description: Some("Show or hide the column with Inspector, Review, and the other panels"),
        owner: Owner::App(Action::Layout(LayoutCommand::ToggleRightColumn)),
    },
];
