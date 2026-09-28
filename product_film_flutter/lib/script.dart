/// The film's scene table — five movements, 4:42.
///
///   I   the question     s01-s03   curiosity
///   II  the old world    s04-s07   need
///   III the studio       s08-s14   relief
///   IV  the engine       s15-s19   confidence
///   V   the proof        s20-s22   invitation
///
/// Durations are the director's cut; the sum is `filmDuration()`.
library;

import 'package:flutter/widgets.dart';

import 'film.dart';
import 'scenes/s01_the_blank.dart' as s01;
import 'scenes/s02_the_distance.dart' as s02;
import 'scenes/s03_the_question.dart' as s03;
import 'scenes/s04_the_weight.dart' as s04;
import 'scenes/s05_the_waste.dart' as s05;
import 'scenes/s06_the_drift.dart' as s06;
import 'scenes/s07_the_promise.dart' as s07;
import 'scenes/s08_the_arrival.dart' as s08;
import 'scenes/s09_the_loop.dart' as s09;
import 'scenes/s10_the_compile.dart' as s10;
import 'scenes/s11_say.dart' as s11;
import 'scenes/s12_live_preview.dart' as s12;
import 'scenes/s13_build_and_run.dart' as s13;
import 'scenes/s14_everything_in_it.dart' as s14;
import 'scenes/s15_under_the_hood.dart' as s15;
import 'scenes/s16_three_trees.dart' as s16;
import 'scenes/s17_the_damage.dart' as s17;
import 'scenes/s18_the_contracts.dart' as s18;
import 'scenes/s19_no_borrowed_weight.dart' as s19;
import 'scenes/s20_the_ledger.dart' as s20;
import 'scenes/s21_beta.dart' as s21;
import 'scenes/s22_endcard.dart' as s22;

const filmScenes = <SceneSpec>[
  // ------------------------------------------------------ I · the question
  SceneSpec(id: 's01', name: 'the_blank', seconds: 11.0, build: s01.build),
  SceneSpec(id: 's02', name: 'the_distance', seconds: 13.0, build: s02.build),
  SceneSpec(id: 's03', name: 'the_question', seconds: 10.0, build: s03.build),
  // ------------------------------------------------------ II · the old world
  SceneSpec(id: 's04', name: 'the_weight', seconds: 14.0, build: s04.build),
  SceneSpec(id: 's05', name: 'the_waste', seconds: 13.0, build: s05.build),
  SceneSpec(id: 's06', name: 'the_drift', seconds: 13.0, build: s06.build),
  SceneSpec(id: 's07', name: 'the_promise', seconds: 15.0, build: s07.build),
  // ------------------------------------------------------ III · the studio
  SceneSpec(id: 's08', name: 'the_arrival', seconds: 10.0, build: s08.build, cut: true),
  SceneSpec(id: 's09', name: 'the_loop', seconds: 16.0, build: s09.build, cut: true),
  SceneSpec(id: 's10', name: 'the_compile', seconds: 14.0, build: s10.build),
  SceneSpec(id: 's11', name: 'say', seconds: 13.0, build: s11.build),
  SceneSpec(id: 's12', name: 'live_preview', seconds: 12.0, build: s12.build),
  SceneSpec(id: 's13', name: 'build_and_run', seconds: 13.0, build: s13.build),
  SceneSpec(id: 's14', name: 'everything_in_it', seconds: 12.0, build: s14.build),
  // ------------------------------------------------------ IV · the engine
  SceneSpec(id: 's15', name: 'under_the_hood', seconds: 10.0, build: s15.build),
  SceneSpec(id: 's16', name: 'three_trees', seconds: 13.0, build: s16.build),
  SceneSpec(id: 's17', name: 'the_damage', seconds: 12.0, build: s17.build),
  SceneSpec(id: 's18', name: 'the_contracts', seconds: 13.0, build: s18.build),
  SceneSpec(id: 's19', name: 'no_borrowed_weight', seconds: 10.0, build: s19.build),
  // ------------------------------------------------------ V · the proof
  SceneSpec(id: 's20', name: 'the_ledger', seconds: 18.0, build: s20.build),
  SceneSpec(id: 's21', name: 'beta', seconds: 12.0, build: s21.build),
  SceneSpec(id: 's22', name: 'the_endcard', seconds: 15.0, build: s22.build),
];
