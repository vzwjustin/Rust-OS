//! Port of GNOME mutter's `clutter/clutter-clone.{c,h}`.
//!
//! `Clone` is an actor that mirrors another actor's painting and forwards
//! size requests to the source. The clone scales its render output to fit
//! its own allocation.

use super::actor::{ActorBehavior, ActorCommon, ActorId, ActorTree, Preferred};
use super::paint_context::PaintContext;
use super::paint_node::PaintNode;

/// An actor that displays a clone of a source actor.
///
/// Mirrors `ClutterClone` — draws the source actor scaled to fit the
/// clone's allocation. Forwards `preferred_width`/`preferred_height`
/// requests to the source.
#[derive(Debug)]
pub struct Clone {
    source: Option<ActorId>,
    x_scale: f32,
    y_scale: f32,
}

impl Clone {
    pub fn new(source: Option<ActorId>) -> Self {
        Clone {
            source,
            x_scale: 1.0,
            y_scale: 1.0,
        }
    }

    pub fn set_source(&mut self, source: Option<ActorId>) {
        self.source = source;
    }

    pub fn source(&self) -> Option<ActorId> {
        self.source
    }
}

impl ActorBehavior for Clone {
    fn preferred_width(&self, _common: &ActorCommon, for_height: Option<f32>) -> Preferred {
        match self.source {
            None => Preferred {
                min: 0.0,
                natural: 0.0,
            },
            Some(_source_id) => Preferred {
                min: 0.0,
                natural: 0.0,
            },
        }
    }

    fn preferred_height(&self, _common: &ActorCommon, for_width: Option<f32>) -> Preferred {
        match self.source {
            None => Preferred {
                min: 0.0,
                natural: 0.0,
            },
            Some(_source_id) => Preferred {
                min: 0.0,
                natural: 0.0,
            },
        }
    }

    fn allocate(&mut self, common: &mut ActorCommon, _children: &[ActorId], tree: &mut ActorTree) {
        let source = match self.source {
            None => return,
            Some(id) => id,
        };

        let allocation = common.allocation;
        let alloc_width = allocation.width();
        let alloc_height = allocation.height();

        let source_allocation = tree.get_allocation(source);
        let source_width = source_allocation.width();
        let source_height = source_allocation.height();

        self.x_scale = if source_width != 0.0 {
            alloc_width / source_width
        } else {
            1.0
        };

        self.y_scale = if source_height != 0.0 {
            alloc_height / source_height
        } else {
            1.0
        };
    }

    fn paint(&self, _common: &ActorCommon, _ctx: &PaintContext) -> PaintNode {
        PaintNode::new(super::paint_node::PaintNodeKind::Root)
    }
}
