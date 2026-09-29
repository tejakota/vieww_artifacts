//! A fetched-and-rendered table — the gap-closure pass's `vieww-network`
//! crate, photographed.
//!
//! A `MemoryClient` with two routes (the test-double that is also the
//! demo's whole backend), a `GET` per route, the bodies rendered as a data
//! table: the request/response/task shape as it looks in an application.
//! The client's request log closes the picture — "it remembers what it
//! was asked" is half of what makes it a test double.

use vieww_foundation::{Color, TextStyle};
use vieww_network::{HttpClient, HttpRequest, HttpResponse, MemoryClient, Url};
use vieww_widget::prelude::*;

/// The routes this "backend" serves: a device list and a status line.
fn client() -> MemoryClient {
    MemoryClient::new()
        .route("/api/devices", |_| {
            HttpResponse::ok(
                "Speaker,Living room,82\nLamp,Bedroom,15\nThermostat,Hall,64",
            )
        })
        .route("/api/status", |_| {
            HttpResponse::ok("ok, uptime 4h 12m, 3 clients")
        })
}

/// GET one route and return its body. A `MemoryClient`'s tasks are ready
/// the moment `send` returns — the synchronous path a test double gives,
/// which is the path a deterministic demo wants.
fn fetch(client: &MemoryClient, path: &str) -> String {
    let url = Url::parse(&format!("https://home.test{path}")).expect("a well-formed URL");
    let task = client.send(HttpRequest::get(url));
    match task.value().ready() {
        Some(response) => String::from_utf8_lossy(&response.body).into_owned(),
        None => String::from("(no route)"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("70 — network", Size::new(640.0, 400.0), |d| {
        let client = client();
        let devices: Vec<Vec<String>> = fetch(&client, "/api/devices")
            .lines()
            .map(|line| line.split(',').map(String::from).collect())
            .collect();
        let status = fetch(&client, "/api/status");
        let log: Vec<String> = client
            .requests()
            .iter()
            .map(|request| request.url.to_string_full())
            .collect();

        let columns = vec![
            DataColumn::new("Device", 130.0),
            DataColumn::new("Room", 150.0),
            DataColumn::new("Battery %", 90.0),
        ];
        let row_count = devices.len();
        // The battery cell, tinted by level: "render what you fetched", not
        // just "fetch".
        let cell = move |row: usize, column: usize| -> WidgetNode {
            let value = devices[row][column].clone();
            if column == 2 {
                let level: f32 = value.parse().unwrap_or(0.0);
                let color = if level < 30.0 {
                    Color::rgb(200, 60, 90)
                } else if level < 50.0 {
                    Color::rgb(230, 150, 40)
                } else {
                    Color::rgb(46, 145, 80)
                };
                return Container::new()
                    .color(color.with_alpha(0x24))
                    .radius(4.0)
                    .padding(EdgeInsets::symmetric(6.0, 4.0))
                    .child(Text::new(value).style(TextStyle {
                        size: 13.0,
                        color,
                        ..TextStyle::default()
                    }))
                    .into();
            }
            Text::new(value).style(TextStyle {
                size: 13.0,
                color: Color::rgb(50, 56, 70),
                ..TextStyle::default()
            })
            .into()
        };

        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    Flex::column()
                        .spacing(14.0)
                        .children(children![
                            Text::new("GET /api/devices → 200").style(TextStyle {
                                size: 13.0,
                                color: Color::rgb(90, 100, 120),
                                ..TextStyle::default()
                            }),
                            DataTable::new(columns, row_count, 40.0, cell),
                            Text::new(format!("GET /api/status → 200: {status}")).style(
                                TextStyle {
                                    size: 12.0,
                                    color: Color::rgb(130, 140, 160),
                                    ..TextStyle::default()
                                },
                            ),
                            Text::new(format!("request log: {} entries, e.g. {}",
                                log.len(),
                                log.first().cloned().unwrap_or_default()))
                            .style(TextStyle {
                                size: 11.0,
                                color: Color::rgb(150, 158, 172),
                                ..TextStyle::default()
                            }),
                        ]),
                ),
        );
    })
}
