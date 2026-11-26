use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use serde_json::json;
use std::{
    collections::VecDeque,
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const BOARD_SIZE: usize = 9;
const CELL_COUNT: usize = BOARD_SIZE * BOARD_SIZE;
const PREVIEW_COUNT: usize = 3;
const MIN_LINE_LENGTH: usize = 5;
const MOVE_SECONDS_PER_CELL: f32 = 0.075;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BallColor {
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
}

impl BallColor {
    const ALL: [Self; 7] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
    ];

    fn random(rng: &mut StdRng) -> Self {
        Self::ALL[rng.gen_range(0..Self::ALL.len())]
    }

    fn rgb(self) -> Color32 {
        match self {
            Self::Red => Color32::from_rgb(232, 72, 85),
            Self::Orange => Color32::from_rgb(245, 145, 55),
            Self::Yellow => Color32::from_rgb(244, 207, 65),
            Self::Green => Color32::from_rgb(77, 190, 115),
            Self::Cyan => Color32::from_rgb(52, 190, 202),
            Self::Blue => Color32::from_rgb(73, 119, 224),
            Self::Purple => Color32::from_rgb(167, 94, 207),
        }
    }
}

struct MoveAnimation {
    color: BallColor,
    path: Vec<usize>,
    started: Instant,
}

#[derive(Clone, Copy, Default)]
struct HudDrawn {
    score: bool,
    preview: bool,
    game_over: bool,
}

struct Game {
    board: [Option<BallColor>; CELL_COUNT],
    selected: Option<usize>,
    next_colors: [BallColor; PREVIEW_COUNT],
    score: u32,
    game_over: bool,
    moving: Option<MoveAnimation>,
    status: String,
    hud_drawn: HudDrawn,
    rng: StdRng,
}

impl Game {
    fn new() -> Self {
        Self::from_seed(rand::random())
    }

    fn from_seed(seed: u64) -> Self {
        let mut game = Self::blank(seed);
        let mut empty: Vec<usize> = (0..CELL_COUNT).collect();
        empty.shuffle(&mut game.rng);
        for index in empty.into_iter().take(5) {
            game.board[index] = Some(BallColor::random(&mut game.rng));
        }
        game
    }

    fn blank(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let next_colors = [
            BallColor::random(&mut rng),
            BallColor::random(&mut rng),
            BallColor::random(&mut rng),
        ];
        Self {
            board: [None; CELL_COUNT],
            selected: None,
            next_colors,
            score: 0,
            game_over: false,
            moving: None,
            status: String::new(),
            hud_drawn: HudDrawn::default(),
            rng,
        }
    }

    fn empty() -> Self {
        Self::blank(7)
    }

    fn find_path(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        if start >= CELL_COUNT
            || goal >= CELL_COUNT
            || start == goal
            || self.board[start].is_none()
            || self.board[goal].is_some()
        {
            return None;
        }

        let mut queue = VecDeque::from([start]);
        let mut visited = [false; CELL_COUNT];
        let mut previous = [usize::MAX; CELL_COUNT];
        visited[start] = true;

        while let Some(current) = queue.pop_front() {
            for neighbor in neighbors(current) {
                if visited[neighbor] || self.board[neighbor].is_some() {
                    continue;
                }
                visited[neighbor] = true;
                previous[neighbor] = current;
                if neighbor == goal {
                    let mut path = vec![goal];
                    let mut step = goal;
                    while step != start {
                        step = previous[step];
                        path.push(step);
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(neighbor);
            }
        }
        None
    }

    fn clear_lines(&mut self) -> Vec<usize> {
        const DIRECTIONS: [(isize, isize); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];
        let mut marked = [false; CELL_COUNT];

        for index in 0..CELL_COUNT {
            let Some(color) = self.board[index] else {
                continue;
            };
            let row = (index / BOARD_SIZE) as isize;
            let col = (index % BOARD_SIZE) as isize;

            for (dr, dc) in DIRECTIONS {
                let previous_is_same = board_index(row - dr, col - dc)
                    .and_then(|previous| self.board[previous])
                    == Some(color);
                if previous_is_same {
                    continue;
                }

                let mut line = Vec::new();
                let mut r = row;
                let mut c = col;
                while let Some(line_index) = board_index(r, c) {
                    if self.board[line_index] != Some(color) {
                        break;
                    }
                    line.push(line_index);
                    r += dr;
                    c += dc;
                }
                if line.len() >= MIN_LINE_LENGTH {
                    for line_index in line {
                        marked[line_index] = true;
                    }
                }
            }
        }

        let cleared: Vec<usize> = marked
            .iter()
            .enumerate()
            .filter_map(|(index, &marked)| marked.then_some(index))
            .collect();
        for &index in &cleared {
            self.board[index] = None;
        }
        cleared
    }

    fn handle_cell_click(&mut self, index: usize) {
        if index >= CELL_COUNT || self.game_over || self.moving.is_some() {
            return;
        }
        if self.board[index].is_some() {
            self.selected = Some(index);
            self.status.clear();
            return;
        }
        let Some(start) = self.selected else {
            self.status = "Select a ball first.".to_string();
            return;
        };
        if let Some(path) = self.find_path(start, index) {
            let color = self.board[start].take().expect("selected cell contains a ball");
            self.moving = Some(MoveAnimation {
                color,
                path,
                started: Instant::now(),
            });
            self.selected = None;
            self.status.clear();
        } else {
            self.status = "No free path to that cell.".to_string();
        }
    }

    fn update_animation(&mut self) {
        let finished = self.moving.as_ref().is_some_and(|moving| {
            let steps = moving.path.len().saturating_sub(1) as f32;
            moving.started.elapsed().as_secs_f32() >= steps * MOVE_SECONDS_PER_CELL
        });
        if !finished {
            return;
        }
        let moving = self.moving.take().expect("finished move exists");
        let destination = *moving.path.last().expect("a move has a destination");
        self.complete_move(moving.color, destination);
    }

    fn complete_move(&mut self, color: BallColor, destination: usize) {
        self.board[destination] = Some(color);
        let cleared = self.clear_lines();
        if !cleared.is_empty() {
            self.award(cleared.len());
            self.status = format!("{} balls cleared!", cleared.len());
        } else {
            self.spawn_preview_balls();
        }
        self.update_game_over();
    }

    fn spawn_preview_balls(&mut self) {
        let colors = self.next_colors;
        let mut empty: Vec<usize> = self
            .board
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| cell.is_none().then_some(index))
            .collect();
        empty.shuffle(&mut self.rng);
        let spawn_count = empty.len().min(PREVIEW_COUNT);
        for (index, color) in empty.into_iter().zip(colors).take(spawn_count) {
            self.board[index] = Some(color);
        }
        self.next_colors = [
            BallColor::random(&mut self.rng),
            BallColor::random(&mut self.rng),
            BallColor::random(&mut self.rng),
        ];
        let cleared = self.clear_lines();
        if !cleared.is_empty() {
            self.award(cleared.len());
            self.status = format!("{} balls cleared!", cleared.len());
        } else if spawn_count > 0 {
            self.status.clear();
        }
        self.update_game_over();
    }

    fn award(&mut self, cleared: usize) {
        self.score += (cleared * 2) as u32;
    }

    fn update_game_over(&mut self) {
        self.game_over = self.board.iter().all(Option::is_some);
        if self.game_over {
            self.selected = None;
            self.status = "Game over — the board is full.".to_string();
        }
    }

    fn moving_position(&self) -> Option<(BallColor, f32, f32)> {
        let moving = self.moving.as_ref()?;
        let steps = moving.path.len().saturating_sub(1);
        if steps == 0 {
            return None;
        }
        let progress = moving.started.elapsed().as_secs_f32() / MOVE_SECONDS_PER_CELL;
        let segment = (progress.floor() as usize).min(steps - 1);
        let t = (progress - segment as f32).clamp(0.0, 1.0);
        let from = moving.path[segment];
        let to = moving.path[segment + 1];
        let from_row = (from / BOARD_SIZE) as f32;
        let from_col = (from % BOARD_SIZE) as f32;
        let to_row = (to / BOARD_SIZE) as f32;
        let to_col = (to % BOARD_SIZE) as f32;
        Some((
            moving.color,
            from_col + (to_col - from_col) * t,
            from_row + (to_row - from_row) * t,
        ))
    }
}

fn board_index(row: isize, col: isize) -> Option<usize> {
    (row >= 0 && row < BOARD_SIZE as isize && col >= 0 && col < BOARD_SIZE as isize)
        .then(|| row as usize * BOARD_SIZE + col as usize)
}

fn neighbors(index: usize) -> Vec<usize> {
    let row = (index / BOARD_SIZE) as isize;
    let col = (index % BOARD_SIZE) as isize;
    [(row - 1, col), (row + 1, col), (row, col - 1), (row, col + 1)]
        .into_iter()
        .filter_map(|(r, c)| board_index(r, c))
        .collect()
}

#[derive(Clone)]
enum LaunchMode {
    Normal,
    Screenshot(PathBuf),
    SelfTest(PathBuf),
}

struct ScreenshotState {
    path: PathBuf,
    frames: u32,
    requested: bool,
}

struct TestStep {
    step: &'static str,
    passed: bool,
    detail: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelfTestPhase {
    BeginMove,
    WaitMove,
    WaitClear,
    ShowGameOver,
}

struct SelfTestState {
    path: PathBuf,
    frames: u32,
    phase: SelfTestPhase,
    steps: Vec<TestStep>,
    move_path_len: usize,
    finish_frame: u32,
}

impl SelfTestState {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            frames: 0,
            phase: SelfTestPhase::BeginMove,
            steps: Vec::new(),
            move_path_len: 0,
            finish_frame: 0,
        }
    }

    fn drive(&mut self, game: &mut Game) {
        self.frames += 1;
        match self.phase {
            SelfTestPhase::BeginMove => self.begin_move(game),
            SelfTestPhase::WaitMove => {
                if game.moving.is_none() {
                    let passed = self.move_path_len >= 2 && game.board[20] == Some(BallColor::Red);
                    self.steps.push(TestStep {
                        step: "move",
                        passed,
                        detail: format!(
                            "BFS moved the ball along {} cells to the chosen destination",
                            self.move_path_len
                        ),
                    });
                    self.begin_clear(game);
                }
            }
            SelfTestPhase::WaitClear => {
                if game.moving.is_none() {
                    let passed = game.score == 10
                        && (36..=40).all(|index| game.board[index].is_none())
                        && game.board.iter().all(Option::is_none);
                    self.steps.push(TestStep {
                        step: "act",
                        passed,
                        detail: "A five-ball line cleared for 10 points and suppressed the three-ball spawn"
                            .to_string(),
                    });
                    self.show_game_over(game);
                }
            }
            SelfTestPhase::ShowGameOver => {}
        }
    }

    fn begin_move(&mut self, game: &mut Game) {
        *game = Game::from_seed(42);
        game.board = [None; CELL_COUNT];
        game.board[0] = Some(BallColor::Red);
        game.next_colors = [BallColor::Blue, BallColor::Green, BallColor::Yellow];
        game.handle_cell_click(0);
        game.handle_cell_click(20);
        self.move_path_len = game.moving.as_ref().map_or(0, |moving| moving.path.len());
        self.phase = SelfTestPhase::WaitMove;
        if self.move_path_len == 0 {
            self.steps.push(TestStep {
                step: "move",
                passed: false,
                detail: "The scripted clicks did not start a BFS move".to_string(),
            });
            self.begin_clear(game);
        }
    }

    fn begin_clear(&mut self, game: &mut Game) {
        game.board = [None; CELL_COUNT];
        game.selected = None;
        game.moving = None;
        game.score = 0;
        game.game_over = false;
        game.status.clear();
        for index in [22, 36, 37, 38, 39] {
            game.board[index] = Some(BallColor::Red);
        }
        game.next_colors = [BallColor::Blue, BallColor::Green, BallColor::Yellow];
        game.handle_cell_click(22);
        game.handle_cell_click(40);
        self.phase = SelfTestPhase::WaitClear;
        if game.moving.is_none() {
            self.steps.push(TestStep {
                step: "act",
                passed: false,
                detail: "The scripted clicks did not start the line-clearing move".to_string(),
            });
            self.show_game_over(game);
        }
    }

    fn show_game_over(&mut self, game: &mut Game) {
        for (index, cell) in game.board.iter_mut().enumerate() {
            *cell = Some(BallColor::ALL[(index + index / BOARD_SIZE * 2) % 7]);
        }
        game.game_over = true;
        game.status = "Game over — the board is full.".to_string();
        self.phase = SelfTestPhase::ShowGameOver;
        self.finish_frame = self.frames + 120;
    }

    fn hud_is_drawn(&mut self, game: &Game) -> bool {
        if self.phase != SelfTestPhase::ShowGameOver || self.frames < self.finish_frame {
            return false;
        }
        let hud = game.hud_drawn;
        self.steps.push(TestStep {
            step: "hud",
            passed: hud.score && hud.preview && hud.game_over,
            detail: "Score, next-three preview, and the full-board game-over message are drawn"
                .to_string(),
        });
        true
    }

    fn timed_out(&mut self) -> bool {
        if self.frames <= 600 {
            return false;
        }
        for name in ["move", "act", "hud"] {
            if !self.steps.iter().any(|step| step.step == name) {
                self.steps.push(TestStep {
                    step: name,
                    passed: false,
                    detail: "The scripted check timed out".to_string(),
                });
            }
        }
        true
    }

    fn write_report(&self) -> Result<(), String> {
        let steps: Vec<_> = self
            .steps
            .iter()
            .map(|step| {
                json!({
                    "step": step.step,
                    "passed": step.passed,
                    "detail": step.detail,
                })
            })
            .collect();
        write_json(&self.path, &json!({"version": 1, "steps": steps}))
    }
}

struct ColorLinesApp {
    game: Game,
    screenshot: Option<ScreenshotState>,
    self_test: Option<SelfTestState>,
}

impl ColorLinesApp {
    fn new(mode: LaunchMode) -> Self {
        let game = match &mode {
            LaunchMode::SelfTest(_) => Game::from_seed(42),
            _ => Game::new(),
        };
        let (screenshot, self_test) = match mode {
            LaunchMode::Normal => (None, None),
            LaunchMode::Screenshot(path) => (
                Some(ScreenshotState {
                    path,
                    frames: 0,
                    requested: false,
                }),
                None,
            ),
            LaunchMode::SelfTest(path) => (None, Some(SelfTestState::new(path))),
        };
        Self {
            game,
            screenshot,
            self_test,
        }
    }

    fn draw_interface(&mut self, ctx: &egui::Context) {
        self.game.hud_drawn = HudDrawn::default();
        egui::TopBottomPanel::top("game_header")
            .exact_height(112.0)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.columns(3, |columns| {
                    columns[0].vertical(|ui| {
                        ui.label(
                            RichText::new("COLOR LINES")
                                .font(FontId::proportional(25.0))
                                .strong()
                                .color(Color32::from_rgb(237, 241, 248)),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(format!("Score  {}", self.game.score))
                                .font(FontId::proportional(23.0))
                                .color(Color32::from_rgb(244, 207, 101)),
                        );
                        self.game.hud_drawn.score = true;
                    });
                    columns[1].vertical(|ui| {
                        ui.label(
                            RichText::new("Next balls")
                                .font(FontId::proportional(17.0))
                                .color(Color32::from_rgb(188, 198, 216)),
                        );
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            for color in self.game.next_colors {
                                let (response, painter) =
                                    ui.allocate_painter(Vec2::splat(35.0), Sense::hover());
                                draw_ball(&painter, response.rect.center(), 13.0, color, false);
                            }
                            self.game.hud_drawn.preview = true;
                        });
                    });
                    columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(22.0);
                        if ui
                            .button(
                                RichText::new("New Game")
                                    .font(FontId::proportional(17.0))
                                    .color(Color32::WHITE),
                            )
                            .clicked()
                        {
                            self.game = Game::new();
                        }
                    });
                });
            });

        egui::TopBottomPanel::bottom("game_help")
            .exact_height(36.0)
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("Make lines of 5 or more  •  Diagonal lines count")
                            .color(Color32::from_rgb(163, 176, 198)),
                    );
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            let message = if self.game.game_over {
                "Game over — start a new game to play again."
            } else if !self.game.status.is_empty() {
                self.game.status.as_str()
            } else if self.game.selected.is_some() {
                "Now choose an empty cell."
            } else {
                "Click a ball, then click an empty cell."
            };
            ui.label(
                RichText::new(message)
                    .font(FontId::proportional(17.0))
                    .color(Color32::from_rgb(194, 204, 220)),
            );
            ui.add_space(6.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                let side = ui.available_width().min(ui.available_height()).max(0.0);
                let (board_rect, response) =
                    ui.allocate_exact_size(Vec2::splat(side), Sense::click());
                self.draw_board(ui, board_rect, &response, ctx);
            });
        });
    }

    fn draw_board(
        &mut self,
        ui: &egui::Ui,
        board_rect: Rect,
        response: &egui::Response,
        ctx: &egui::Context,
    ) {
        let painter = ui.painter();
        painter.rect_filled(
            board_rect,
            CornerRadius::same(10),
            Color32::from_rgb(37, 47, 65),
        );
        let cell_size = board_rect.width() / BOARD_SIZE as f32;
        let hovered = response
            .hover_pos()
            .and_then(|position| point_to_cell(board_rect, position));

        for index in 0..CELL_COUNT {
            let rect = cell_rect(board_rect, index);
            let row = index / BOARD_SIZE;
            let col = index % BOARD_SIZE;
            let base = if (row + col) % 2 == 0 {
                Color32::from_rgb(79, 92, 112)
            } else {
                Color32::from_rgb(74, 87, 106)
            };
            let fill = if hovered == Some(index) && !self.game.game_over {
                Color32::from_rgb(94, 108, 129)
            } else {
                base
            };
            painter.rect_filled(
                rect.shrink(2.0),
                CornerRadius::same(7),
                fill,
            );
            painter.rect_stroke(
                rect.shrink(2.0),
                CornerRadius::same(7),
                Stroke::new(1.0_f32, Color32::from_rgb(103, 116, 135)),
                egui::StrokeKind::Inside,
            );

            if let Some(color) = self.game.board[index] {
                let selected = self.game.selected == Some(index);
                draw_ball(
                    painter,
                    rect.center(),
                    cell_size * if selected { 0.39 } else { 0.36 },
                    color,
                    selected,
                );
                if selected {
                    let pulse = (ctx.input(|input| input.time).sin() as f32 + 1.0) * 0.5;
                    painter.circle_stroke(
                        rect.center(),
                        cell_size * (0.43 + pulse * 0.025),
                        Stroke::new(2.5_f32, Color32::from_rgb(255, 224, 128)),
                    );
                }
            }
        }

        if let Some((color, col, row)) = self.game.moving_position() {
            let center = Pos2::new(
                board_rect.left() + (col + 0.5) * cell_size,
                board_rect.top() + (row + 0.5) * cell_size,
            );
            draw_ball(painter, center, cell_size * 0.36, color, false);
        }

        if response.clicked() {
            if let Some(index) = hovered {
                self.game.handle_cell_click(index);
            }
        }

        if self.game.game_over {
            let overlay = Rect::from_center_size(board_rect.center(), Vec2::new(360.0, 175.0));
            painter.rect_filled(
                overlay,
                CornerRadius::same(16),
                Color32::from_rgba_unmultiplied(19, 25, 37, 238),
            );
            painter.rect_stroke(
                overlay,
                CornerRadius::same(16),
                Stroke::new(2.0_f32, Color32::from_rgb(244, 207, 101)),
                egui::StrokeKind::Inside,
            );
            painter.text(
                overlay.center() - Vec2::new(0.0, 43.0),
                Align2::CENTER_CENTER,
                "Game Over",
                FontId::proportional(36.0),
                Color32::from_rgb(255, 225, 139),
            );
            painter.text(
                overlay.center() + Vec2::new(0.0, 8.0),
                Align2::CENTER_CENTER,
                format!("Final score: {}", self.game.score),
                FontId::proportional(23.0),
                Color32::WHITE,
            );
            painter.text(
                overlay.center() + Vec2::new(0.0, 51.0),
                Align2::CENTER_CENTER,
                "The board is full",
                FontId::proportional(17.0),
                Color32::from_rgb(188, 198, 216),
            );
            self.game.hud_drawn.game_over = true;
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(screenshot) = self.screenshot.as_mut() else {
            return;
        };
        screenshot.frames += 1;
        if screenshot.frames >= 120 && !screenshot.requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            screenshot.requested = true;
        }
        if screenshot.requested {
            let picture = ctx.input(|input| {
                input.raw.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(picture) = picture {
                if let Err(error) = save_screenshot(&screenshot.path, &picture) {
                    eprintln!("Could not save screenshot: {error}");
                    std::process::exit(1);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        ctx.request_repaint();
    }

    fn handle_self_test(&mut self, ctx: &egui::Context) {
        if let Some(self_test) = self.self_test.as_mut() {
            self_test.drive(&mut self.game);
        }

        self.draw_interface(ctx);

        let should_finish = if let Some(self_test) = self.self_test.as_mut() {
            self_test.hud_is_drawn(&self.game) || self_test.timed_out()
        } else {
            false
        };
        if should_finish {
            let result = self
                .self_test
                .as_ref()
                .expect("self-test state exists")
                .write_report();
            if let Err(error) = result {
                eprintln!("Could not save self-test report: {error}");
                std::process::exit(1);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl eframe::App for ColorLinesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.game.update_animation();
        if self.self_test.is_some() {
            self.handle_self_test(ctx);
        } else {
            self.draw_interface(ctx);
            self.handle_screenshot(ctx);
        }
        if self.game.moving.is_some() || self.game.selected.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn cell_rect(board_rect: Rect, index: usize) -> Rect {
    let cell_size = board_rect.width() / BOARD_SIZE as f32;
    let row = index / BOARD_SIZE;
    let col = index % BOARD_SIZE;
    Rect::from_min_size(
        Pos2::new(
            board_rect.left() + col as f32 * cell_size,
            board_rect.top() + row as f32 * cell_size,
        ),
        Vec2::splat(cell_size),
    )
}

fn point_to_cell(board_rect: Rect, point: Pos2) -> Option<usize> {
    if !board_rect.contains(point) {
        return None;
    }
    let cell_size = board_rect.width() / BOARD_SIZE as f32;
    let col = ((point.x - board_rect.left()) / cell_size) as usize;
    let row = ((point.y - board_rect.top()) / cell_size) as usize;
    (row < BOARD_SIZE && col < BOARD_SIZE).then_some(row * BOARD_SIZE + col)
}

fn draw_ball(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: BallColor,
    selected: bool,
) {
    painter.circle_filled(
        center + Vec2::new(0.0, radius * 0.16),
        radius * 1.02,
        Color32::from_rgba_unmultiplied(12, 18, 28, 85),
    );
    painter.circle_filled(center, radius, color.rgb());
    painter.circle_filled(
        center + Vec2::new(0.0, radius * 0.24),
        radius * 0.76,
        Color32::from_rgba_unmultiplied(0, 0, 0, 24),
    );
    painter.circle_filled(
        center - Vec2::new(radius * 0.27, radius * 0.30),
        radius * 0.30,
        Color32::from_rgba_unmultiplied(255, 255, 255, 105),
    );
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(
            if selected { 2.5_f32 } else { 1.3_f32 },
            if selected {
                Color32::from_rgb(255, 229, 151)
            } else {
                Color32::from_rgba_unmultiplied(20, 26, 40, 125)
            },
        ),
    );
}

fn save_screenshot(path: &Path, picture: &egui::ColorImage) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let [width, height] = picture.size;
    image::save_buffer(
        path,
        picture.as_raw(),
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|error| error.to_string())
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let data = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, data).map_err(|error| error.to_string())
}

fn parse_launch_mode() -> Result<LaunchMode, String> {
    let mut args = env::args_os().skip(1);
    let Some(option) = args.next() else {
        return Ok(LaunchMode::Normal);
    };
    let value = args
        .next()
        .ok_or_else(|| format!("Missing file path after {}", option.to_string_lossy()))?;
    if args.next().is_some() {
        return Err("Too many arguments".to_string());
    }
    match option.to_string_lossy().as_ref() {
        "--screenshot" => Ok(LaunchMode::Screenshot(PathBuf::from(value))),
        "--self-test" => Ok(LaunchMode::SelfTest(PathBuf::from(value))),
        _ => Err(format!(
            "Unknown option: {}. Use --screenshot <file.png> or --self-test <file.json>",
            option.to_string_lossy()
        )),
    }
}

fn main() {
    let mode = match parse_launch_mode() {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let passive = !matches!(mode, LaunchMode::Normal);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Color Lines")
        .with_inner_size([650.0, 770.0])
        .with_min_inner_size([500.0, 620.0])
        .with_active(!passive);
    #[cfg(target_os = "windows")]
    if passive {
        viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if passive {
        use eframe::winit::platform::macos::EventLoopBuilderExtMacOS;
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_activate_ignoring_other_apps(false);
        }));
    }

    if let Err(error) = eframe::run_native(
        "Color Lines",
        options,
        Box::new(move |_creation_context| Ok(Box::new(ColorLinesApp::new(mode)))),
    ) {
        eprintln!("Could not start Color Lines: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_edges_do_not_create_out_of_bounds_indices() {
        assert_eq!(board_index(-1, 0), None);
        assert_eq!(board_index(0, -1), None);
        assert_eq!(board_index(BOARD_SIZE as isize, 0), None);
        assert_eq!(board_index(0, BOARD_SIZE as isize), None);
        assert_eq!(board_index(0, 0), Some(0));
    }

    #[test]
    fn bfs_finds_a_free_path_and_rejects_a_blocked_one() {
        let mut game = Game::empty();
        game.board[0] = Some(BallColor::Red);
        let path = game.find_path(0, 20).expect("a free path should exist");
        assert_eq!(path.first(), Some(&0));
        assert_eq!(path.last(), Some(&20));
        assert!(path.windows(2).all(|pair| {
            pair[0].abs_diff(pair[1]) == 1 || pair[0].abs_diff(pair[1]) == BOARD_SIZE
        }));

        game.board[1] = Some(BallColor::Blue);
        game.board[9] = Some(BallColor::Blue);
        assert_eq!(game.find_path(0, 20), None);
    }

    #[test]
    fn clears_horizontal_vertical_and_diagonal_lines_without_double_scoring() {
        let mut game = Game::empty();
        for (row, col) in [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
        ] {
            game.board[row * BOARD_SIZE + col] = Some(BallColor::Green);
        }
        let mut cleared = game.clear_lines();
        cleared.sort_unstable();
        assert_eq!(cleared, vec![0, 1, 2, 3, 4, 10, 20, 30, 40]);
        assert!(cleared.iter().all(|&index| game.board[index].is_none()));
    }

    #[test]
    fn a_scoring_move_awards_points_and_does_not_spawn() {
        let mut game = Game::empty();
        for index in [36, 37, 38, 39] {
            game.board[index] = Some(BallColor::Red);
        }
        game.complete_move(BallColor::Red, 40);
        assert_eq!(game.score, 10);
        assert!(game.board.iter().all(Option::is_none));
    }

    #[test]
    fn a_non_scoring_move_spawns_exactly_the_preview_colors() {
        let mut game = Game::empty();
        game.next_colors = [BallColor::Blue, BallColor::Green, BallColor::Yellow];
        game.complete_move(BallColor::Red, 20);
        assert_eq!(game.board.iter().flatten().count(), 4);
        for color in [BallColor::Blue, BallColor::Green, BallColor::Yellow] {
            assert!(game.board.iter().any(|cell| *cell == Some(color)));
        }
    }
}
