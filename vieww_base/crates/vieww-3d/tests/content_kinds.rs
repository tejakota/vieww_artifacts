//! The fourth-pass content kinds, end to end through `Renderer`:
//! skinned meshes, point clouds, clustered (meshlet) meshes and custom
//! fragment shaders each leave their own count in `RenderStats` and pixels
//! in the image.

use vieww_3d::geometry::{box_mesh, sphere};
use vieww_3d::shader::FragmentShader;
use vieww_3d::{Camera, Content, Light, Material, Node, Renderer, Rgb, Scene, SkinnedMesh, Vec3};
use vieww_foundation::Color;
use vieww_mesh::gltf::MorphTarget;

fn base() -> Scene {
    let mut s = Scene::new();
    s.background = Rgb::BLACK;
    s.add(
        Node::new(
            "amb",
            Content::Light(Light::Ambient {
                color: Rgb::WHITE,
                intensity: 1.0,
            }),
        ),
        None,
    );
    s
}

fn cam() -> Camera {
    Camera::perspective(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 0.8)
}

fn lit(img: &vieww_foundation::Image) -> usize {
    img.pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 10 || p[1] > 10 || p[2] > 10)
        .count()
}

#[test]
fn a_morphing_mesh_moves_with_its_weights() {
    let mesh = box_mesh(1.0, 1.0, 1.0);
    let n = mesh.positions.len();
    let target = MorphTarget {
        positions: vec![[1.5, 0.0, 0.0]; n],
        normals: Vec::new(),
    };
    let mut s = base();
    let id = s.add(
        Node::new(
            "m",
            Content::skinned(
                SkinnedMesh::morphing(mesh, vec![target]),
                Material::basic(Color::WHITE),
            ),
        ),
        None,
    );
    let r = Renderer::new(64, 48).samples(1);
    let (a, st) = r.render(&mut s, &cam());
    assert_eq!(st.skinned_vertices, n);
    if let Content::Skinned { morph_weights, .. } = &mut s.node_mut(id).content {
        *morph_weights = vec![1.0];
    }
    let (b, _) = r.render(&mut s, &cam());
    assert_ne!(a.pixels(), b.pixels());
}

#[test]
fn a_point_cloud_draws_splats() {
    let mut s = base();
    let pts: Vec<Vec3> = (0..200)
        .map(|i| {
            let a = i as f32 * 0.31;
            Vec3::new(a.cos() * 1.5, (i as f32 / 100.0) - 1.0, a.sin() * 1.5)
        })
        .collect();
    s.add(
        Node::new(
            "pc",
            Content::points(pts, vec![Rgb::new(1.0, 0.5, 0.2)], 0.03),
        ),
        None,
    );
    let (img, st) = Renderer::new(96, 64).samples(1).render(&mut s, &cam());
    assert!(st.points > 150, "{st:?}");
    assert!(lit(&img) > 150);
}

#[test]
fn clustered_meshes_cull_their_back_half() {
    let mut s = base();
    s.add(
        Node::new(
            "c",
            Content::clustered(&sphere(1.0, 64, 32), 3, Material::lambert(Color::WHITE)),
        ),
        None,
    );
    let (img, st) = Renderer::new(96, 64).samples(1).render(&mut s, &cam());
    assert!(st.clusters > 0);
    assert!(st.clusters_culled * 3 > st.clusters, "{st:?}");
    assert!(lit(&img) > 500);
}

#[test]
fn a_custom_shader_runs_per_fragment() {
    let mut s = base();
    let stripes = FragmentShader::new("stripes", |f| {
        let k = if (f.world.y * 10.0).floor() as i32 % 2 == 0 {
            1.0
        } else {
            0.0
        };
        [k, 0.0, 1.0 - k, 1.0]
    });
    s.add(
        Node::new(
            "b",
            Content::mesh(
                box_mesh(2.0, 2.0, 2.0),
                Material::basic(Color::WHITE).shader(stripes),
            ),
        ),
        None,
    );
    let (img, st) = Renderer::new(64, 48).samples(1).render(&mut s, &cam());
    assert!(st.shaded_custom > 100);
    let px = img.pixels();
    let reds = px
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 200 && p[2] < 50)
        .count();
    let blues = px
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[2] > 200 && p[0] < 50)
        .count();
    assert!(reds > 50 && blues > 50);
}
