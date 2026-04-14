use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::WindowResolution,
};

// Simulation grid size.
// (A small size gives a "pixel art" effect, a bigger one demands more CPU computation)
const SIM_WIDTH: usize = 800;
const SIM_HEIGHT: usize = 600;

fn setup_scene(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // --------------------------------------------------------
    // 1. DYNAMIC TEXTURE CREATION
    // --------------------------------------------------------

    // A image filled with a base color is created
    let image = Image::new_fill(
        Extent3d {
            width: SIM_WIDTH as u32,
            height: SIM_HEIGHT as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 50, 150, 255],            // Dark blue in RGBA
        TextureFormat::Rgba8UnormSrgb, // Standard format for pixel screen
        // This allows `update_waves_system`
        // to modify the pixels (MAIN_WORLD) and the camera to display them (RENDER_WORLD).
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    // Let's add this image to Bevy asset manager to get a handle
    let texture_handle = images.add(image);

    // --------------------------------------------------------
    // 2. CAMERA AND DISPLAYING
    // --------------------------------------------------------

    // A basic 2D camera at the center of the screen
    commands.spawn(Camera2d);

    // Let's spawn a Sprite as a "canva" to display our texture
    commands.spawn(Sprite::from_image(texture_handle.clone()));

    // --------------------------------------------------------
    // 3. SIMULATION RESOURCE INIT
    // --------------------------------------------------------

    // Let's calculate the total number of cells in our grid
    let num_cells = SIM_WIDTH * SIM_HEIGHT;

    commands.insert_resource(WaveSimulation {
        width: SIM_WIDTH,
        height: SIM_HEIGHT,
        buffer_current: vec![0.0; num_cells],
        buffer_previous: vec![0.0; num_cells],
        obstacles: vec![false; num_cells],
        texture_handle,
    });
}

// Our global resource containing the simulation state
#[derive(Resource)]
struct WaveSimulation {
    width: usize,
    height: usize,
    buffer_current: Vec<f32>,
    buffer_previous: Vec<f32>,
    obstacles: Vec<bool>,
    texture_handle: Handle<Image>, // Reference to the image displayed on the screen
}

impl WaveSimulation {
    // Utility function to get the 1D index from the 2D coordinates
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
}

fn update_waves_system(mut wave_sim: ResMut<WaveSimulation>, mut images: ResMut<Assets<Image>>) {
    let w = wave_sim.width;
    let h = wave_sim.height;
    let damping = 0.995; // Damping to stop the wave

    // 1. Let's calculate the new frame (wave equation)
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let idx = y * w + x;

            // Ignore when an obstacle
            if wave_sim.obstacles[idx] {
                continue;
            }

            // Add the 4 neighbors to the previous frame
            let neighbors = wave_sim.buffer_previous[(y - 1) * w + x]
                + wave_sim.buffer_previous[(y + 1) * w + x]
                + wave_sim.buffer_previous[y * w + (x - 1)]
                + wave_sim.buffer_previous[y * w + (x + 1)];

            // Wave algorithm : (Neighbors / 2) - current position
            let mut new_height = (neighbors / 2.0) - wave_sim.buffer_current[idx];
            new_height *= damping;

            wave_sim.buffer_current[idx] = new_height;
        }
    }

    // 2. Buffer swapping
    let tmp_buf = wave_sim.buffer_current.clone();
    wave_sim.buffer_current = wave_sim.buffer_previous.clone();
    wave_sim.buffer_previous = tmp_buf.clone();

    // 3. Bevy texture update
    if let Some(image) = images.get_mut(&wave_sim.texture_handle) {
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let height = wave_sim.buffer_previous[idx];

                // Height conversion (-1.0 à 1.0) into RGBA (0 à 255)
                let color_val = ((height + 1.0) / 2.0 * 255.0).clamp(0.0, 255.0) as u8;

                let pixel_idx = idx * 4;
                if wave_sim.obstacles[idx] {
                    // Obstacles are in red for instance
                    image.data.as_mut().unwrap()[pixel_idx..pixel_idx + 4]
                        .copy_from_slice(&[255, 0, 0, 255]);
                } else {
                    // Water is shaded with white/blue
                    image.data.as_mut().unwrap()[pixel_idx..pixel_idx + 4]
                        .copy_from_slice(&[color_val, color_val, 255, 255]);
                }
            }
        }
    }
}

fn mouse_interaction_system(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    // Let's get the camera and its position in the world
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut wave_sim: ResMut<WaveSimulation>,
) {
    // If no pressed button, nothing is done to spare computation
    if !buttons.pressed(MouseButton::Left) && !buttons.pressed(MouseButton::Right) {
        return;
    }

    let window = windows.single();
    let (camera, camera_transform) = camera_q.single().unwrap();

    // 1. Position on the Window (in pixels, (0,0) top left)
    if let Some(cursor_pos) = window.unwrap().cursor_position() {
        // 2. Conversion into Bevy's 2D World
        if let Ok(world_pos) = camera.viewport_to_world_2d(camera_transform, cursor_pos) {
            let w = wave_sim.width as f32;
            let h = wave_sim.height as f32;

            // 3. Conversion from the World into our local Grid
            let grid_x_f32 = world_pos.x + (w / 2.0);
            let grid_y_f32 = (h / 2.0) - world_pos.y;

            // 4. Let's check if the click is inside the window
            if grid_x_f32 >= 0.0 && grid_x_f32 < w && grid_y_f32 >= 0.0 && grid_y_f32 < h {
                let grid_x = grid_x_f32 as usize;
                let grid_y = grid_y_f32 as usize;

                // 5. Algorithm security
                // The algorithm needs to read the 4 neighbors (top, bottom, left, right).
                // The very first and the very last row/column are forbidden
                // to prevent the error "out of bounds".
                if grid_x > 0
                    && grid_x < wave_sim.width - 1
                    && grid_y > 0
                    && grid_y < wave_sim.height - 1
                {
                    let idx = wave_sim.index(grid_x, grid_y);

                    if buttons.pressed(MouseButton::Left) {
                        // Left Click : A disturbance by forcing an extreme height.
                        wave_sim.buffer_previous[idx] = 250.0;
                    } else if buttons.pressed(MouseButton::Right) {
                        // Right Click : A permanent obstacle is set.
                        wave_sim.obstacles[idx] = true;
                    }
                }
            }
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Simulation d'Ondes".into(),
                resolution: WindowResolution::new(800, 600),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (update_waves_system, mouse_interaction_system))
        .run();
}
