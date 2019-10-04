use serde::{ Serialize, Deserialize };
use libtetris::*;
use rand::prelude::*;
use crate::{ Controller, GameConfig };

pub struct Game {
    pub board: Board<ColoredRow>,
    state: GameState,
    config: GameConfig,
    did_hold: bool,
    prev: Controller,
    used: Controller,
    das_delay: u32,
    pub garbage_queue: u32,
    attacking: u32
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Event {
    PieceSpawned { new_in_queue: Piece },
    SpawnDelayStart,
    PieceMoved,
    PieceRotated,
    PieceTSpined,
    PieceHeld(Piece),
    StackTouched,
    SoftDropped,
    PieceFalling(FallingPiece, FallingPiece),
    EndOfLineClearDelay,
    PiecePlaced {
        piece: FallingPiece,
        locked: LockResult,
        hard_drop_distance: Option<i32>
    },
    GarbageSent(u32),
    GarbageAdded(Vec<usize>),
    GameOver
}

enum GameState {
    SpawnDelay(u32),
    LineClearDelay(u32),
    Falling(FallingState),
    GameOver
}

#[derive(Copy, Clone, Debug)]
struct FallingState {
    piece: FallingPiece,
    lowest_y: i32,
    rotation_move_count: u32,
    gravity: i32,
    lock_delay: u32,
    soft_drop_delay: u32
}

impl Game {
    pub fn new(config: GameConfig, piece_rng: &mut impl Rng) -> Self {
        let mut board = Board::new();
        for _ in 0..config.next_queue_size {
            board.add_next_piece(board.generate_next_piece(piece_rng));
        }
        Game {
            board, config,
            prev: Default::default(),
            used: Default::default(),
            did_hold: false,
            das_delay: config.delayed_auto_shift,
            state: GameState::SpawnDelay(config.spawn_delay),
            garbage_queue: 0,
            attacking: 0
        }
    }

    pub fn update(
        &mut self, current: Controller, piece_rng: &mut impl Rng, garbage_rng: &mut impl Rng
    ) -> Vec<Event> {
        update_input(&mut self.used.left, self.prev.left, current.left);
        update_input(&mut self.used.right, self.prev.right, current.right);
        update_input(&mut self.used.rotate_right, self.prev.rotate_right, current.rotate_right);
        update_input(&mut self.used.rotate_left, self.prev.rotate_left, current.rotate_left);
        update_input(&mut self.used.soft_drop, self.prev.soft_drop, current.soft_drop);
        update_input(&mut self.used.hold, self.prev.hold, current.hold);
        self.used.hard_drop = !self.prev.hard_drop && current.hard_drop;
        self.used.soft_drop = current.soft_drop;

        let switched_left_right = (current.left != self.prev.left) &&
            (current.right != self.prev.right);

        if current.left != current.right && !switched_left_right {
            if self.used.left || self.used.right {
                // While movement is buffered, don't let the time
                // until the next shift fall below the auto-repeat rate.
                // Otherwise we might rapidly shift twice when a piece spawns.
                if self.das_delay > self.config.auto_repeat_rate {
                    self.das_delay -= 1;
                }
            } else if self.das_delay == 0 {
                // Apply auto-shift
                self.das_delay = self.config.auto_repeat_rate;
                self.used.left = current.left;
                self.used.right = current.right;
            } else {
                self.das_delay -= 1;
            }
        } else {
            // Reset delayed auto shift
            self.das_delay = self.config.delayed_auto_shift;
            self.used.left = false;
            self.used.right = false;

            // Redo button presses
            if current.left && !self.prev.left {
                self.used.left = true;
            } else if current.right && !self.prev.right {
                self.used.right = true;
            }
        }

        self.prev = current;

        match self.state {
            GameState::SpawnDelay(0) => {
                let next_piece = self.board.advance_queue().unwrap();
                let new_piece = self.board.generate_next_piece(piece_rng);
                self.board.add_next_piece(new_piece);
                if let Some(spawned) = FallingPiece::spawn(next_piece, &self.board) {
                    self.state = GameState::Falling(FallingState {
                        piece: spawned,
                        lowest_y: spawned.cells().into_iter().map(|(_,y,_)| y).min().unwrap(),
                        rotation_move_count: 0,
                        gravity: self.config.gravity,
                        lock_delay: 30,
                        soft_drop_delay: 0
                    });
                    let mut ghost = spawned;
                    ghost.sonic_drop(&self.board);
                    vec![
                        Event::PieceSpawned { new_in_queue: new_piece },
                        Event::PieceFalling(spawned, ghost)
                    ]
                } else {
                    self.state = GameState::GameOver;
                    vec![Event::GameOver]
                }
            }
            GameState::SpawnDelay(ref mut delay) => {
                *delay -= 1;
                if *delay + 1 == self.config.spawn_delay {
                    vec![Event::SpawnDelayStart]
                } else {
                    vec![]
                }
            }
            GameState::LineClearDelay(0) => {
                self.state = GameState::SpawnDelay(self.config.spawn_delay);
                let mut events = vec![Event::EndOfLineClearDelay];
                self.deal_garbage(&mut events, garbage_rng);
                events
            }
            GameState::LineClearDelay(ref mut delay) => {
                *delay -= 1;
                vec![]
            }
            GameState::GameOver => vec![Event::GameOver],
            GameState::Falling(ref mut falling) => {
                let mut events = vec![];
                let was_on_stack = self.board.on_stack(&falling.piece);

                // Hold
                if !self.did_hold && self.used.hold {
                    self.did_hold = true;
                    events.push(Event::PieceHeld(falling.piece.kind.0));
                    if let Some(piece) = self.board.hold(falling.piece.kind.0) {
                        // Piece in hold; the piece spawns instantly
                        if let Some(spawned) = FallingPiece::spawn(piece, &self.board) {
                            *falling = FallingState {
                                piece: spawned,
                                lowest_y: spawned.cells().into_iter().map(|(_,y,_)| y).min().unwrap(),
                                rotation_move_count: 0,
                                gravity: self.config.gravity,
                                lock_delay: 30,
                                soft_drop_delay: 0
                            };
                            let mut ghost = spawned;
                            ghost.sonic_drop(&self.board);
                            events.push(Event::PieceFalling(spawned, ghost));
                        } else {
                            // Hold piece couldn't spawn; Block Out
                            self.state = GameState::GameOver;
                            events.push(Event::GameOver);
                        }
                    } else {
                        // Nothing in hold; spawn next piece normally
                        self.state = GameState::SpawnDelay(self.config.spawn_delay);
                    }
                    return events;
                }

                // Rotate
                if self.used.rotate_right {
                    if falling.piece.cw(&self.board) {
                        self.used.rotate_right = false;
                        falling.rotation_move_count += 1;
                        falling.lock_delay = self.config.lock_delay;
                        if falling.piece.tspin != TspinStatus::None {
                            events.push(Event::PieceTSpined);
                        } else {
                            events.push(Event::PieceRotated);
                        }
                    }
                }
                if self.used.rotate_left {
                    if falling.piece.ccw(&self.board) {
                        self.used.rotate_left = false;
                        falling.rotation_move_count += 1;
                        falling.lock_delay = self.config.lock_delay;
                        if falling.piece.tspin != TspinStatus::None {
                            events.push(Event::PieceTSpined);
                        } else {
                            events.push(Event::PieceRotated);
                        }
                    }
                }

                // Shift
                if self.used.left {
                    if falling.piece.shift(&self.board, -1, 0) {
                        self.used.left = false;
                        falling.rotation_move_count += 1;
                        falling.lock_delay = self.config.lock_delay;
                        events.push(Event::PieceMoved);
                    }
                }
                if self.used.right {
                    if falling.piece.shift(&self.board, 1, 0) {
                        self.used.right = false;
                        falling.rotation_move_count += 1;
                        falling.lock_delay = self.config.lock_delay;
                        events.push(Event::PieceMoved);
                    }
                }

                // 15 move lock rule reset
                let low_y = falling.piece.cells().into_iter().map(|(_,y,_)| y).min().unwrap();
                if low_y < falling.lowest_y {
                    falling.rotation_move_count = 0;
                    falling.lowest_y = low_y;
                }

                // 15 move lock rule
                if falling.rotation_move_count >= self.config.move_lock_rule {
                    let mut p = falling.piece;
                    p.sonic_drop(&self.board);
                    let low_y = p.cells().into_iter().map(|(_,y,_)| y).min().unwrap();
                    // I don't think the 15 move lock rule applies if the piece can fall to a lower
                    // y position than it has ever reached before.
                    if low_y >= falling.lowest_y {
                        let mut f = *falling;
                        f.piece = p;
                        self.lock(f, &mut events, garbage_rng, None);
                        return events;
                    }
                }

                // Hard drop
                if self.used.hard_drop {
                    let y = falling.piece.y;
                    falling.piece.sonic_drop(&self.board);
                    let distance = y - falling.piece.y;
                    let f = *falling;
                    self.lock(f, &mut events, garbage_rng, Some(distance));
                    return events;
                }

                if self.board.on_stack(&falling.piece) {
                    // Lock delay
                    if !was_on_stack {
                        events.push(Event::StackTouched);
                    }
                    falling.lock_delay -= 1;
                    falling.gravity = self.config.gravity;
                    if falling.lock_delay == 0 {
                        let f = *falling;
                        self.lock(f, &mut events, garbage_rng, None);
                        return events;
                    }
                } else {
                    // Gravity
                    falling.lock_delay = self.config.lock_delay;
                    falling.gravity -= 100;
                    while falling.gravity < 0 {
                        falling.gravity += self.config.gravity;
                        falling.piece.shift(&self.board, 0, -1);
                    }

                    if self.board.on_stack(&falling.piece) {
                        events.push(Event::StackTouched);
                    } else if self.config.gravity > self.config.soft_drop_speed as i32 * 100 {
                        // Soft drop
                        if self.used.soft_drop {
                            if falling.soft_drop_delay == 0 {
                                falling.piece.shift(&self.board, 0, -1);
                                falling.soft_drop_delay = self.config.soft_drop_speed;
                                falling.gravity = self.config.gravity;
                                events.push(Event::PieceMoved);
                                if self.board.on_stack(&falling.piece) {
                                    events.push(Event::StackTouched);
                                }
                                events.push(Event::SoftDropped);
                            } else {
                                falling.soft_drop_delay -= 1;
                            }
                        } else {
                            falling.soft_drop_delay = 0;
                        }
                    }
                }

                let mut ghost = falling.piece;
                ghost.sonic_drop(&self.board);
                events.push(Event::PieceFalling(falling.piece, ghost));

                events
            }
        }
    }

    fn lock(
        &mut self,
        falling: FallingState,
        events: &mut Vec<Event>,
        garbage_rng: &mut impl Rng,
        dist: Option<i32>
    ) {
        self.did_hold = false;
        let locked = self.board.lock_piece(falling.piece);;

        events.push(Event::PiecePlaced {
            piece: falling.piece,
            locked: locked.clone(),
            hard_drop_distance: dist
        });

        if locked.locked_out {
            self.state = GameState::GameOver;
            events.push(Event::GameOver);
        } else if locked.cleared_lines.is_empty() {
            self.state = GameState::SpawnDelay(self.config.spawn_delay);
            self.deal_garbage(events, garbage_rng);
        } else {
            self.attacking += locked.garbage_sent;
            self.state = GameState::LineClearDelay(self.config.line_clear_delay);
        }
    }

    fn deal_garbage(&mut self, events: &mut Vec<Event>, rng: &mut impl Rng) {
        if self.attacking > self.garbage_queue {
            self.attacking -= self.garbage_queue;
            self.garbage_queue = 0;
        } else {
            self.garbage_queue -= self.attacking;
            self.attacking = 0;
        }
        if self.garbage_queue > 0 {
            let mut dead = false;
            let mut col = rng.gen_range(0, 10);
            let mut garbage_columns = vec![];
            for _ in 0..self.garbage_queue.min(self.config.max_garbage_add) {
                if rng.gen_bool(1.0/3.0) {
                    col = rng.gen_range(0, 10);
                }
                garbage_columns.push(col);
                dead |= self.board.add_garbage(col);
            }
            self.garbage_queue -= self.garbage_queue.min(self.config.max_garbage_add);
            events.push(Event::GarbageAdded(garbage_columns));
            if dead {
                events.push(Event::GameOver);
                self.state = GameState::GameOver;
            }
        } else if self.attacking > 0 {
            events.push(Event::GarbageSent(self.attacking));
            self.attacking = 0;
        }
    }
}

fn update_input(used: &mut bool, prev: bool, current: bool) {
    if !current {
        *used = false
    } else if !prev {
        *used = true;
    }
}