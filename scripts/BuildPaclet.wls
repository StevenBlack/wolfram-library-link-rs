#!/usr/bin/env wolframscript
(* ::Package:: *)

(* ::Title:: *)
(*Build Paclet*)


Needs["PacletTools`"]
Needs["CodeFormatter`"]


(* ::Subsubsection:: *)
(*Copy LibraryResources libraries*)


$dir = FileNameJoin[{"RustLink", "LibraryResources", $SystemID}];


CreateDirectory[$dir]


Scan[
	exampleLib |-> CopyFile[
		FileNameJoin[{"target", "debug", "examples", exampleLib}],
		FileNameJoin[{$dir, exampleLib}]
	],
	{
		(* Library Tests *)
		"liblibrary_tests.dylib",
		(* Examples *)
		"libbasic_types.dylib",
		"libraw_wstp_function.dylib",
		"libbasic_expressions.dylib",
		"libmanaged_exprs.dylib",
		"libnumeric_arrays.dylib",
		"libraw_librarylink_function.dylib",
		"libasync_file_watcher_raw.dylib",
		"libasync_file_watcher.dylib",
		"libdata_store.dylib",
		"libaborts.dylib",
		"libwstp_example.dylib"
	}
]


(* ::Subsubsection:: *)
(*Build the paclet*)


result = PacletBuild[FileNameJoin[{Directory[], "RustLink"}]];

If[FailureQ[result],
	Echo @ CodeFormat @ ToString[result, InputForm];
	Exit[-1];
];
