//! The boxes of a page's row whose pieces are laid out here (an effect's
//! settings, a mouse's own settings), as the window's row of boxes (BoxRow,
//! ui/system/layout.slint) shows them: the window's model of the boxes, each
//! box's own model of its pieces (kept, so a piece in use is never rebuilt
//! under the pointer), and how wide the row made each box, which its pieces
//! are laid out to.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

use crate::{Cell, LaidBox};

/// A box to show: its title, the least width the row grows it from, how tall
/// its pieces are, and its pieces.
pub struct Laid {
    pub title: String,
    pub least: f32,
    pub h: f32,
    pub cells: Vec<Cell>,
}

#[derive(Default)]
pub struct Row {
    pub model: Rc<VecModel<LaidBox>>,
    cells: Vec<Rc<VecModel<Cell>>>,
    widths: Rc<RefCell<Vec<f32>>>,
}

impl Row {
    /// How wide the row made each box, kept where the window's word of it
    /// lands as it's drawn (see [noted]), so the next layout has it.
    pub fn widths(&self) -> Rc<RefCell<Vec<f32>>> {
        self.widths.clone()
    }

    /// How wide box `i` is: as the row made it, or its least until the row has drawn it.
    pub fn width(&self, i: usize, least: f32) -> f32 {
        self.widths.borrow().get(i).copied().filter(|w| *w > 0.0).unwrap_or(least)
    }

    /// The boxes shown, in place: each box's pieces brought to its new ones,
    /// and a box changed only where its title, least width or height did.
    pub fn show(&mut self, boxes: Vec<Laid>) {
        while self.cells.len() < boxes.len() {
            self.cells.push(Rc::new(VecModel::default()));
        }
        let mut rows = Vec::new();
        for (i, b) in boxes.into_iter().enumerate() {
            crate::front::sync_cells(&self.cells[i], b.cells);
            rows.push(LaidBox { title: b.title.into(), least: b.least, h: b.h, cells: ModelRc::from(self.cells[i].clone()) });
        }
        for (i, r) in rows.iter().enumerate() {
            if i < self.model.row_count() {
                let old = self.model.row_data(i);
                let same = old.as_ref().is_some_and(|o| o.title == r.title && o.least == r.least && o.h == r.h);
                if !same {
                    self.model.set_row_data(i, r.clone());
                }
            } else {
                self.model.push(r.clone());
            }
        }
        while self.model.row_count() > rows.len() {
            self.model.remove(self.model.row_count() - 1);
        }
    }
}

/// The window says the row made box `i` `w` wide; whether that's new (and its
/// pieces need laying out anew).
pub fn noted(widths: &RefCell<Vec<f32>>, i: usize, w: f32) -> bool {
    let mut ws = widths.borrow_mut();
    if ws.len() <= i {
        ws.resize(i + 1, 0.0);
    }
    let new = (ws[i] - w).abs() >= 0.5;
    ws[i] = w;
    new
}
