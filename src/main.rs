use macroquad::prelude::*;
use std::collections::VecDeque;

const W: usize = 10;
const H: usize = 20;
const HID: usize = 2; // скрытые строки сверху
const ROWS: usize = H + HID;
const LOCK_DELAY: f32 = 0.5;
const MAX_RESETS: u32 = 15;

// Порядок: I, O, T, S, Z, J, L. Координаты внутри рамки фигуры (y вниз).
const BASE: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)],
    [(1, 0), (2, 0), (1, 1), (2, 1)],
    [(1, 0), (0, 1), (1, 1), (2, 1)],
    [(1, 0), (2, 0), (0, 1), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
    [(0, 0), (0, 1), (1, 1), (2, 1)],
    [(2, 0), (0, 1), (1, 1), (2, 1)],
];
const SIZE: [i32; 7] = [4, 4, 3, 3, 3, 3, 3];
const COLORS: [Color; 7] = [SKYBLUE, YELLOW, PURPLE, GREEN, RED, BLUE, ORANGE];

// Состояния поворота: 0 = спавн, 1 = R (по часовой), 2, 3 = L.
fn cells_of(kind: usize, rot: usize) -> [(i32, i32); 4] {
    let n = SIZE[kind];
    let mut c = BASE[kind];
    if kind != 1 {
        for _ in 0..rot {
            c = c.map(|(x, y)| (n - 1 - y, x));
        }
    }
    c
}

// Таблицы пинков SRS (y вверх, как в стандарте).
const OFF: [[(i32, i32); 5]; 4] = [
    [(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    [(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
];

fn kicks(kind: usize, from: usize, to: usize) -> [(i32, i32); 5] {
    if kind == 0 {
        match (from, to) {
            (0, 1) => [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
            (1, 0) => [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
            (1, 2) => [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
            (2, 1) => [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
            (2, 3) => [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
            (3, 2) => [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
            (3, 0) => [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
            _ => [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
        }
    } else {
        let mut k = [(0, 0); 5];
        for i in 0..5 {
            k[i] = (OFF[from][i].0 - OFF[to][i].0, OFF[from][i].1 - OFF[to][i].1);
        }
        k
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Left,
    Right,
    Cw,
    Ccw,
    SoftDrop,
    HardDrop,
    Hold,
}

// ---------- Жесты (как в официальной версии) ----------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Undecided,
    Horiz,
    Down,
    Up,
}

struct Gesture {
    id: u64,
    start: Vec2,
    last: Vec2,
    t0: f64,
    mode: Mode,
    acc: f32,
}

impl Gesture {
    fn new(id: u64, pos: Vec2) -> Self {
        Gesture { id, start: pos, last: pos, t0: get_time(), mode: Mode::Undecided, acc: 0.0 }
    }

    fn update(&mut self, pos: Vec2, cell: f32, out: &mut Vec<Action>) {
        let d = pos - self.last;
        self.last = pos;
        let total = pos - self.start;

        if self.mode == Mode::Undecided {
            if total.length() > cell * 0.6 {
                self.mode = if total.x.abs() > total.y.abs() {
                    Mode::Horiz
                } else if total.y > 0.0 {
                    Mode::Down
                } else {
                    Mode::Up
                };
                self.acc = 0.0;
            }
            return;
        }

        match self.mode {
            Mode::Horiz => {
                self.acc += d.x;
                while self.acc >= cell {
                    out.push(Action::Right);
                    self.acc -= cell;
                }
                while self.acc <= -cell {
                    out.push(Action::Left);
                    self.acc += cell;
                }
            }
            Mode::Down => {
                self.acc += d.y;
                while self.acc >= cell {
                    out.push(Action::SoftDrop);
                    self.acc -= cell;
                }
                if self.acc < 0.0 {
                    self.acc = 0.0;
                }
            }
            _ => {}
        }
    }

    fn finish(&self, pos: Vec2, cell: f32, sh: f32, out: &mut Vec<Action>) {
        let total = pos - self.start;
        let dur = ((get_time() - self.t0) as f32).max(0.01);
        match self.mode {
            Mode::Undecided => {
                if dur < 0.35 {
                    // тап: правая половина экрана — по часовой, левая — против
                    if pos.x < screen_width() / 2.0 {
                        out.push(Action::Ccw);
                    } else {
                        out.push(Action::Cw);
                    }
                }
            }
            Mode::Down => {
                if total.y > cell * 2.0 && total.y / dur > sh * 1.5 {
                    out.push(Action::HardDrop);
                }
            }
            Mode::Up => {
                if total.y < -cell * 2.0 {
                    out.push(Action::Hold);
                }
            }
            Mode::Horiz => {}
        }
    }
}

// ---------- Игра ----------

struct Game {
    board: [[u8; W]; ROWS],
    kind: usize,
    rot: usize,
    x: i32,
    y: i32,
    queue: VecDeque<usize>,
    bag: Vec<usize>,
    hold: Option<usize>,
    can_hold: bool,
    score: u32,
    lines: u32,
    level: u32,
    fall: f32,
    lock_t: f32,
    resets: u32,
    lowest: i32,
    last_rot: bool,
    last_kick: usize,
    b2b: bool,
    combo: i32,
    over: bool,
    over_t: f32,
    popup: Vec<String>,
    popup_t: f32,
}

impl Game {
    fn new() -> Self {
        let mut g = Game {
            board: [[0; W]; ROWS],
            kind: 0,
            rot: 0,
            x: 3,
            y: 0,
            queue: VecDeque::new(),
            bag: Vec::new(),
            hold: None,
            can_hold: true,
            score: 0,
            lines: 0,
            level: 1,
            fall: 0.0,
            lock_t: 0.0,
            resets: 0,
            lowest: 0,
            last_rot: false,
            last_kick: 0,
            b2b: false,
            combo: -1,
            over: false,
            over_t: 0.0,
            popup: Vec::new(),
            popup_t: 0.0,
        };
        for _ in 0..6 {
            let k = g.next_from_bag();
            g.queue.push_back(k);
        }
        g.spawn_next();
        g
    }

    // Генератор «7-bag»: каждая фигура выпадает раз в 7 штук.
    fn next_from_bag(&mut self) -> usize {
        if self.bag.is_empty() {
            self.bag = (0..7).collect();
            for i in (1..7).rev() {
                let j = rand::gen_range(0, i as i32 + 1) as usize;
                self.bag.swap(i, j);
            }
        }
        self.bag.pop().unwrap()
    }

    fn fits(&self, kind: usize, rot: usize, x: i32, y: i32) -> bool {
        cells_of(kind, rot).iter().all(|&(cx, cy)| {
            let bx = x + cx;
            let by = y + cy;
            bx >= 0
                && bx < W as i32
                && by >= 0
                && by < ROWS as i32
                && self.board[by as usize][bx as usize] == 0
        })
    }

    fn occupied(&self, x: i32, y: i32) -> bool {
        if x < 0 || x >= W as i32 || y < 0 || y >= ROWS as i32 {
            return true;
        }
        self.board[y as usize][x as usize] != 0
    }

    fn abs_cells(&self) -> [(i32, i32); 4] {
        cells_of(self.kind, self.rot).map(|(cx, cy)| (self.x + cx, self.y + cy))
    }

    fn grounded(&self) -> bool {
        !self.fits(self.kind, self.rot, self.x, self.y + 1)
    }

    fn spawn(&mut self, kind: usize) {
        self.kind = kind;
        self.rot = 0;
        self.x = 3;
        self.y = 0;
        self.last_rot = false;
        self.lock_t = 0.0;
        self.resets = 0;
        self.fall = 0.0;
        if !self.fits(kind, 0, self.x, self.y) {
            self.over = true;
            return;
        }
        // как в гайдлайне: сразу опускается на строку после появления
        if self.fits(kind, 0, self.x, self.y + 1) {
            self.y += 1;
        }
        self.lowest = self.y;
    }

    fn spawn_next(&mut self) {
        let k = self.queue.pop_front().unwrap();
        let nk = self.next_from_bag();
        self.queue.push_back(nk);
        self.spawn(k);
        self.can_hold = true;
    }

    fn hold_piece(&mut self) {
        if !self.can_hold {
            return;
        }
        let cur = self.kind;
        match self.hold.take() {
            Some(h) => self.spawn(h),
            None => self.spawn_next(),
        }
        self.hold = Some(cur);
        self.can_hold = false;
    }

    // Сброс таймера фиксации при движении/повороте (до 15 раз на одной высоте).
    fn on_manipulate(&mut self) {
        if self.grounded() && self.resets < MAX_RESETS {
            self.lock_t = 0.0;
            self.resets += 1;
        }
    }

    fn move_by(&mut self, dx: i32) {
        if self.fits(self.kind, self.rot, self.x + dx, self.y) {
            self.x += dx;
            self.last_rot = false;
            self.on_manipulate();
        }
    }

    fn rotate(&mut self, cw: bool) {
        if self.kind == 1 {
            return;
        }
        let to = if cw { (self.rot + 1) % 4 } else { (self.rot + 3) % 4 };
        for (i, &(kx, ky)) in kicks(self.kind, self.rot, to).iter().enumerate() {
            let nx = self.x + kx;
            let ny = self.y - ky;
            if self.fits(self.kind, to, nx, ny) {
                self.rot = to;
                self.x = nx;
                self.y = ny;
                self.last_rot = true;
                self.last_kick = i;
                if self.y > self.lowest {
                    self.lowest = self.y;
                    self.resets = 0;
                }
                self.on_manipulate();
                return;
            }
        }
    }

    fn soft_down(&mut self) -> bool {
        if self.fits(self.kind, self.rot, self.x, self.y + 1) {
            self.y += 1;
            self.last_rot = false;
            if self.y > self.lowest {
                self.lowest = self.y;
                self.resets = 0;
            }
            true
        } else {
            false
        }
    }

    fn ghost_y(&self) -> i32 {
        let mut gy = self.y;
        while self.fits(self.kind, self.rot, self.x, gy + 1) {
            gy += 1;
        }
        gy
    }

    fn hard_drop(&mut self) {
        let mut n = 0;
        while self.soft_down() {
            n += 1;
        }
        self.score += 2 * n;
        self.lock();
    }

    fn lock(&mut self) {
        // T-spin
        let mut tspin = 0u8; // 1 = mini, 2 = обычный
        if self.kind == 2 && self.last_rot {
            let corners = [(0, 0), (2, 0), (0, 2), (2, 2)];
            let cnt = corners.iter().filter(|&&(cx, cy)| self.occupied(self.x + cx, self.y + cy)).count();
            if cnt >= 3 {
                let front = match self.rot {
                    0 => [(0, 0), (2, 0)],
                    1 => [(2, 0), (2, 2)],
                    2 => [(0, 2), (2, 2)],
                    _ => [(0, 0), (0, 2)],
                };
                let front_full = front.iter().all(|&(cx, cy)| self.occupied(self.x + cx, self.y + cy));
                tspin = if front_full || self.last_kick == 4 { 2 } else { 1 };
            }
        }

        let cells = self.abs_cells();
        let mut all_hidden = true;
        for &(cx, cy) in cells.iter() {
            self.board[cy as usize][cx as usize] = self.kind as u8 + 1;
            if cy >= HID as i32 {
                all_hidden = false;
            }
        }

        // Очистка линий
        let mut new_board = [[0u8; W]; ROWS];
        let mut r = ROWS;
        let mut n = 0usize;
        for row in (0..ROWS).rev() {
            if self.board[row].iter().all(|&c| c != 0) {
                n += 1;
            } else {
                r -= 1;
                new_board[r] = self.board[row];
            }
        }
        self.board = new_board;

        // Очки
        let lvl = self.level;
        let mut pts = 0u32;
        let mut difficult = false;
        let mut label = String::new();
        match (tspin, n) {
            (2, 0) => {
                pts = 400;
                label = "T-SPIN".into();
            }
            (1, 0) => {
                pts = 100;
                label = "T-SPIN MINI".into();
            }
            (1, 1) => {
                pts = 200;
                difficult = true;
                label = "T-SPIN MINI SINGLE".into();
            }
            (1, 2) => {
                pts = 400;
                difficult = true;
                label = "T-SPIN MINI DOUBLE".into();
            }
            (2, 1) => {
                pts = 800;
                difficult = true;
                label = "T-SPIN SINGLE".into();
            }
            (2, 2) => {
                pts = 1200;
                difficult = true;
                label = "T-SPIN DOUBLE".into();
            }
            (2, 3) => {
                pts = 1600;
                difficult = true;
                label = "T-SPIN TRIPLE".into();
            }
            (_, 1) => {
                pts = 100;
                label = "SINGLE".into();
            }
            (_, 2) => {
                pts = 300;
                label = "DOUBLE".into();
            }
            (_, 3) => {
                pts = 500;
                label = "TRIPLE".into();
            }
            (_, 4) => {
                pts = 800;
                difficult = true;
                label = "TETRIS".into();
            }
            _ => {}
        }

        let mut popup: Vec<String> = Vec::new();
        if n > 0 {
            if difficult {
                if self.b2b {
                    pts = pts * 3 / 2;
                    label = format!("B2B {}", label);
                }
                self.b2b = true;
            } else {
                self.b2b = false;
            }
            self.combo += 1;
            if self.combo >= 1 {
                self.score += 50 * self.combo as u32 * lvl;
            }
        } else {
            self.combo = -1;
        }
        self.score += pts * lvl;
        if !label.is_empty() {
            popup.push(label);
        }
        if n > 0 && self.combo >= 1 {
            popup.push(format!("COMBO x{}", self.combo));
        }
        if n > 0 && self.board.iter().all(|row| row.iter().all(|&c| c == 0)) {
            self.score += [800, 1200, 1800, 2000][n - 1] * lvl;
            popup.push("PERFECT CLEAR".into());
        }
        if !popup.is_empty() {
            self.popup = popup;
            self.popup_t = 1.6;
        }

        self.lines += n as u32;
        self.level = self.lines / 10 + 1;

        if all_hidden {
            self.over = true;
            return;
        }
        self.spawn_next();
    }

    fn gravity(&self) -> f32 {
        let l = (self.level - 1) as f32;
        (0.8 - l * 0.007).powf(l).max(0.0005)
    }

    fn tick(&mut self, dt: f32) {
        if self.popup_t > 0.0 {
            self.popup_t -= dt;
        }

        self.fall += dt;
        let g = self.gravity();
        for _ in 0..ROWS {
            if self.fall < g {
                break;
            }
            self.fall -= g;
            if !self.soft_down() {
                self.fall = 0.0;
                break;
            }
        }

        if self.grounded() {
            self.lock_t += dt;
            if self.lock_t >= LOCK_DELAY {
                self.lock();
            }
        } else {
            self.lock_t = 0.0;
        }
    }
}

// ---------- Отрисовка ----------

fn draw_mini(kind: usize, x: f32, y: f32, size: f32, color: Color) {
    let cells = cells_of(kind, 0);
    let minx = cells.iter().map(|c| c.0).min().unwrap();
    let maxx = cells.iter().map(|c| c.0).max().unwrap();
    let miny = cells.iter().map(|c| c.1).min().unwrap();
    let maxy = cells.iter().map(|c| c.1).max().unwrap();
    let mc = size / 4.5;
    let w = (maxx - minx + 1) as f32 * mc;
    let h = (maxy - miny + 1) as f32 * mc;
    let ox = x + (size - w) / 2.0;
    let oy = y + (size - h) / 2.0;
    for &(cx, cy) in cells.iter() {
        draw_rectangle(
            ox + (cx - minx) as f32 * mc + 1.0,
            oy + (cy - miny) as f32 * mc + 1.0,
            mc - 2.0,
            mc - 2.0,
            color,
        );
    }
}

fn draw_centered(text: &str, cx: f32, y: f32, size: f32, color: Color) {
    let d = measure_text(text, None, size as u16, 1.0);
    draw_text(text, cx - d.width / 2.0, y, size, color);
}

fn conf() -> Conf {
    Conf {
        window_title: "Tetris".to_owned(),
        window_width: 450,
        window_height: 800,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    rand::srand(macroquad::miniquad::date::now() as u64);
    let mut g = Game::new();
    let mut gesture: Option<Gesture> = None;

    loop {
        let dt = get_frame_time().min(0.05);
        let sw = screen_width();
        let sh = screen_height();
        let cell = (sw * 0.62 / W as f32).min(sh * 0.88 / H as f32);
        let bw = cell * W as f32;
        let bx = (sw - bw) / 2.0;
        let by = (sh - cell * H as f32) / 2.0;

        let mut actions: Vec<Action> = Vec::new();

        for t in touches() {
            match t.phase {
                TouchPhase::Started => {
                    if gesture.is_none() {
                        gesture = Some(Gesture::new(t.id, t.position));
                    }
                }
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if let Some(gs) = gesture.as_mut() {
                        if gs.id == t.id {
                            gs.update(t.position, cell, &mut actions);
                        }
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    if gesture.as_ref().map_or(false, |gs| gs.id == t.id) {
                        let gs = gesture.take().unwrap();
                        if t.phase == TouchPhase::Ended {
                            gs.finish(t.position, cell, sh, &mut actions);
                        }
                    }
                }
            }
        }

        // Клавиатура (для проверки на компьютере)
        if is_key_pressed(KeyCode::Left) {
            actions.push(Action::Left);
        }
        if is_key_pressed(KeyCode::Right) {
            actions.push(Action::Right);
        }
        if is_key_pressed(KeyCode::Up) {
            actions.push(Action::Cw);
        }
        if is_key_pressed(KeyCode::Z) {
            actions.push(Action::Ccw);
        }
        if is_key_pressed(KeyCode::Down) {
            actions.push(Action::SoftDrop);
        }
        if is_key_pressed(KeyCode::Space) {
            actions.push(Action::HardDrop);
        }
        if is_key_pressed(KeyCode::C) {
            actions.push(Action::Hold);
        }

        if g.over {
            g.over_t += dt;
            if g.over_t > 0.6 && !actions.is_empty() {
                g = Game::new();
            }
        } else {
            for a in actions {
                match a {
                    Action::Left => g.move_by(-1),
                    Action::Right => g.move_by(1),
                    Action::Cw => g.rotate(true),
                    Action::Ccw => g.rotate(false),
                    Action::SoftDrop => {
                        if g.soft_down() {
                            g.score += 1;
                        }
                    }
                    Action::HardDrop => g.hard_drop(),
                    Action::Hold => g.hold_piece(),
                }
                if g.over {
                    break;
                }
            }
            if !g.over {
                g.tick(dt);
            }
        }

        // ----- Рисуем -----
        clear_background(Color::new(0.07, 0.07, 0.1, 1.0));
        let panel = Color::new(0.14, 0.15, 0.2, 1.0);
        let fs = (cell * 0.5).max(12.0);

        // Поле
        draw_rectangle(bx, by, bw, cell * H as f32, Color::new(0.11, 0.11, 0.15, 1.0));
        for i in 1..W {
            draw_line(bx + i as f32 * cell, by, bx + i as f32 * cell, by + cell * H as f32, 1.0, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        for i in 1..H {
            draw_line(bx, by + i as f32 * cell, bx + bw, by + i as f32 * cell, 1.0, Color::new(1.0, 1.0, 1.0, 0.04));
        }

        for row in HID..ROWS {
            for col in 0..W {
                let c = g.board[row][col];
                if c != 0 {
                    draw_rectangle(
                        bx + col as f32 * cell + 1.0,
                        by + (row - HID) as f32 * cell + 1.0,
                        cell - 2.0,
                        cell - 2.0,
                        COLORS[(c - 1) as usize],
                    );
                }
            }
        }

        if !g.over {
            let gy = g.ghost_y();
            for &(cx, cy) in cells_of(g.kind, g.rot).iter() {
                let y = gy + cy;
                if y >= HID as i32 {
                    draw_rectangle_lines(
                        bx + (g.x + cx) as f32 * cell + 2.0,
                        by + (y - HID as i32) as f32 * cell + 2.0,
                        cell - 4.0,
                        cell - 4.0,
                        2.0,
                        COLORS[g.kind],
                    );
                }
            }
            for &(cx, cy) in cells_of(g.kind, g.rot).iter() {
                let y = g.y + cy;
                if y >= HID as i32 {
                    draw_rectangle(
                        bx + (g.x + cx) as f32 * cell + 1.0,
                        by + (y - HID as i32) as f32 * cell + 1.0,
                        cell - 2.0,
                        cell - 2.0,
                        COLORS[g.kind],
                    );
                }
            }
        }

        // Левая панель: HOLD + статистика
        let box_size = (bx - 10.0).min(cell * 3.2).max(20.0);
        let lx = bx - box_size - 6.0;
        draw_rectangle(lx, by, box_size, box_size, panel);
        draw_text("HOLD", lx + 4.0, by + fs, fs * 0.8, GRAY);
        if let Some(h) = g.hold {
            let c = if g.can_hold { COLORS[h] } else { DARKGRAY };
            draw_mini(h, lx, by + fs * 0.4, box_size, c);
        }

        let mut sy = by + box_size + cell * 1.2;
        for (label, value) in [
            ("SCORE", g.score.to_string()),
            ("LEVEL", g.level.to_string()),
            ("LINES", g.lines.to_string()),
        ] {
            draw_text(label, lx + 2.0, sy, fs * 0.8, GRAY);
            draw_text(&value, lx + 2.0, sy + fs * 1.1, fs * 1.1, WHITE);
            sy += fs * 3.0;
        }

        // Правая панель: очередь NEXT
        let rx = bx + bw + 6.0;
        draw_text("NEXT", rx + 2.0, by + fs * 0.8, fs * 0.8, GRAY);
        let nb = box_size * 0.85;
        for (i, &k) in g.queue.iter().take(5).enumerate() {
            let y = by + fs + i as f32 * (nb + 4.0);
            draw_rectangle(rx, y, nb, nb, panel);
            draw_mini(k, rx, y, nb, COLORS[k]);
        }

        // Всплывающие надписи (SINGLE, T-SPIN DOUBLE, COMBO...)
        if g.popup_t > 0.0 {
            let a = (g.popup_t / 0.4).min(1.0);
            for (i, line) in g.popup.iter().enumerate() {
                draw_centered(
                    line,
                    sw / 2.0,
                    by + cell * 7.0 + i as f32 * cell * 1.1,
                    cell * 0.85,
                    Color::new(1.0, 1.0, 1.0, a),
                );
            }
        }

        if g.over {
            draw_rectangle(bx, by + cell * 7.0, bw, cell * 5.0, Color::new(0.0, 0.0, 0.0, 0.88));
            draw_centered("GAME OVER", sw / 2.0, by + cell * 9.0, cell * 1.4, RED);
            draw_centered(&format!("Score {}", g.score), sw / 2.0, by + cell * 10.4, cell * 0.9, WHITE);
            draw_centered("Tap to restart", sw / 2.0, by + cell * 11.6, cell * 0.8, GRAY);
        }

        next_frame().await;
    }
}
