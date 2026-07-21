use x11rb::connection::Connection;
use x11rb::errors::ConnectionError;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::config::{Config, Side};

pub struct PanelWindow {
    pub conn: RustConnection,
    #[allow(dead_code)]
    pub screen_width: u16,
    #[allow(dead_code)]
    pub screen_height: u16,
    pub panel_win: u32,
    pub trigger_win: u32,
    pub gc: u32,
    pub depth: u8,
    pub panel_width: u16,
    #[allow(dead_code)]
    pub side: Side,
    pub visible_x: i32,
    pub hidden_x: i32,
}

impl PanelWindow {
    pub fn create(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let screen_width = screen.width_in_pixels;
        let screen_height = screen.height_in_pixels;

        // Find 32-bit TrueColor visual
        let depth: u8 = 32;
        let visual = screen
            .allowed_depths
            .iter()
            .find(|d| d.depth == depth)
            .and_then(|d| d.visuals.iter().find(|v| v.class == VisualClass::TRUE_COLOR))
            .ok_or("No 32-bit TrueColor visual available")?;

        let visual_id = visual.visual_id;

        // Create colormap for 32-bit visual
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual_id)?;

        // Position calculations
        let (visible_x, hidden_x, trigger_x) = match config.side {
            Side::Right => (
                (screen_width - config.panel_width) as i32,
                screen_width as i32,
                (screen_width - config.trigger_width) as i32,
            ),
            Side::Left => (
                0i32,
                -(config.panel_width as i32),
                0i32,
            ),
        };

        // --- Panel window (32-bit, override-redirect) ---
        let panel_win = conn.generate_id()?;
        conn.create_window(
            depth,
            panel_win,
            screen.root,
            hidden_x as i16,
            0,
            config.panel_width,
            screen_height,
            0,
            WindowClass::INPUT_OUTPUT,
            visual_id,
            &CreateWindowAux::new()
                .override_redirect(1)
                .colormap(colormap)
                .background_pixel(0)
                .border_pixel(0)
                .event_mask(EventMask::EXPOSURE | EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW),
        )?;

        // --- Trigger window (input-only, 1px at edge) ---
        let trigger_win = conn.generate_id()?;
        conn.create_window(
            0, // CopyFromParent depth for InputOnly
            trigger_win,
            screen.root,
            trigger_x as i16,
            0,
            config.trigger_width,
            screen_height,
            0,
            WindowClass::INPUT_ONLY,
            0, // CopyFromParent visual
            &CreateWindowAux::new()
                .override_redirect(1)
                .event_mask(EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW),
        )?;

        // --- Graphics context for put_image ---
        let gc = conn.generate_id()?;
        conn.create_gc(gc, panel_win, &CreateGCAux::new().graphics_exposures(0))?;

        // Map both windows
        conn.map_window(panel_win)?;
        conn.map_window(trigger_win)?;
        conn.flush()?;

        Ok(Self {
            conn,
            screen_width,
            screen_height,
            panel_win,
            trigger_win,
            gc,
            depth,
            panel_width: config.panel_width,
            side: config.side,
            visible_x,
            hidden_x,
        })
    }

    pub fn set_x(&self, x: i32) -> Result<(), ConnectionError> {
        self.conn
            .configure_window(self.panel_win, &ConfigureWindowAux::new().x(x))?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn put_image(&self, data: &[u8]) -> Result<(), ConnectionError> {
        self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            self.panel_win,
            self.gc,
            self.panel_width,
            self.screen_height,
            0,
            0,
            0,
            self.depth,
            data,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn raise_panel(&self) -> Result<(), ConnectionError> {
        self.conn.configure_window(
            self.panel_win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// Poll for next event without blocking.
    pub fn poll_event(&self) -> Result<Option<Event>, ConnectionError> {
        self.conn.poll_for_event()
    }
}