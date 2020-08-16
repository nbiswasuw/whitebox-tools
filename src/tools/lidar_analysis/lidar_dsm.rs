/*
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 16/08/2020
Last Modified: 16/08/2020
License: MIT
*/

use self::na::Vector3;
use crate::algorithms::{point_in_poly, triangulate};
use crate::lidar::*;
use crate::na;
use crate::raster::*;
use crate::spatial_ref_system::esri_wkt_from_epsg;
use crate::structures::{BoundingBox, Point2D, DistanceMetric, FixedRadiusSearch2D};
use crate::tools::*;
use num_cpus;
use std::io::{Error, ErrorKind};
use std::sync::mpsc;
use std::sync::Arc;
use std::{env, f64, fs, path, thread};
// use rayon::prelude::*;

pub struct LidarDigitalSurfaceModel {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl LidarDigitalSurfaceModel {
    pub fn new() -> LidarDigitalSurfaceModel {
        // public constructor
        let name = "LidarDigitalSurfaceModel".to_string();
        let toolbox = "LiDAR Tools".to_string();
        let description = "Creates a top-surface digital surface model (DSM) from a LiDAR point cloud.".to_string();

        let mut parameters = vec![];
        parameters.push(ToolParameter {
            name: "Input File".to_owned(),
            flags: vec!["-i".to_owned(), "--input".to_owned()],
            description: "Input LiDAR file (including extension).".to_owned(),
            parameter_type: ParameterType::ExistingFile(ParameterFileType::Lidar),
            default_value: None,
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Output File".to_owned(),
            flags: vec!["-o".to_owned(), "--output".to_owned()],
            description: "Output raster file (including extension).".to_owned(),
            parameter_type: ParameterType::NewFile(ParameterFileType::Raster),
            default_value: None,
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Grid Resolution".to_owned(),
            flags: vec!["--resolution".to_owned()],
            description: "Output raster's grid resolution.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: Some("1.0".to_owned()),
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Search Radius".to_owned(),
            flags: vec!["--radius".to_owned()],
            description: "Search Radius.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: Some("0.5".to_owned()),
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Minimum Elevation Value (optional)".to_owned(),
            flags: vec!["--minz".to_owned()],
            description: "Optional minimum elevation for inclusion in interpolation.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: None,
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Maximum Elevation Value (optional)".to_owned(),
            flags: vec!["--maxz".to_owned()],
            description: "Optional maximum elevation for inclusion in interpolation.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: None,
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Maximum Triangle Edge Length (optional)".to_owned(),
            flags: vec!["--max_triangle_edge_length".to_owned()],
            description: "Optional maximum triangle edge length; triangles larger than this size will not be gridded.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: None,
            optional: true,
        });

        let sep: String = path::MAIN_SEPARATOR.to_string();
        let p = format!("{}", env::current_dir().unwrap().display());
        let e = format!("{}", env::current_exe().unwrap().display());
        let mut short_exe = e
            .replace(&p, "")
            .replace(".exe", "")
            .replace(".", "")
            .replace(&sep, "");
        if e.contains(".exe") {
            short_exe += ".exe";
        }
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" -i=file.las -o=outfile.tif --returns=last --resolution=2.0 --exclude_cls='3,4,5,6,7,18' --max_triangle_edge_length=5.0", short_exe, name).replace("*", &sep);

        LidarDigitalSurfaceModel {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for LidarDigitalSurfaceModel {
    fn get_source_file(&self) -> String {
        String::from(file!())
    }

    fn get_tool_name(&self) -> String {
        self.name.clone()
    }

    fn get_tool_description(&self) -> String {
        self.description.clone()
    }

    fn get_tool_parameters(&self) -> String {
        let mut s = String::from("{\"parameters\": [");
        for i in 0..self.parameters.len() {
            if i < self.parameters.len() - 1 {
                s.push_str(&(self.parameters[i].to_string()));
                s.push_str(",");
            } else {
                s.push_str(&(self.parameters[i].to_string()));
            }
        }
        s.push_str("]}");
        s
    }

    fn get_example_usage(&self) -> String {
        self.example_usage.clone()
    }

    fn get_toolbox(&self) -> String {
        self.toolbox.clone()
    }

    fn run<'a>(
        &self,
        args: Vec<String>,
        working_directory: &'a str,
        verbose: bool,
    ) -> Result<(), Error> {
        let mut input_file: String = "".to_string();
        let mut output_file: String = "".to_string();
        let mut grid_res: f64 = 1.0;
        let mut search_radius = 0.5f64; 
        let mut max_z = f64::INFINITY;
        let mut min_z = f64::NEG_INFINITY;
        let mut max_triangle_edge_length = f64::INFINITY;

        // read the arguments
        if args.len() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Tool run with no parameters.",
            ));
        }
        for i in 0..args.len() {
            let mut arg = args[i].replace("\"", "");
            arg = arg.replace("\'", "");
            let cmd = arg.split("="); // in case an equals sign was used
            let vec = cmd.collect::<Vec<&str>>();
            let mut keyval = false;
            if vec.len() > 1 {
                keyval = true;
            }
            let flag_val = vec[0].to_lowercase().replace("--", "-");
            if flag_val == "-i" || flag_val == "-input" {
                input_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-o" || flag_val == "-output" {
                output_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-resolution" {
                grid_res = if keyval {
                    vec[1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                } else {
                    args[i + 1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                };
            } else if flag_val == "-radius" {
                search_radius = if keyval {
                    vec[1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                } else {
                    args[i + 1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                };
            } else if flag_val == "-minz" {
                min_z = if keyval {
                    vec[1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                } else {
                    args[i + 1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                };
            } else if flag_val == "-maxz" {
                max_z = if keyval {
                    vec[1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                } else {
                    args[i + 1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                };
            } else if flag_val == "-max_triangle_edge_length" {
                max_triangle_edge_length = if keyval {
                    vec[1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                } else {
                    args[i + 1]
                        .to_string()
                        .parse::<f64>()
                        .expect(&format!("Error parsing {}", flag_val))
                };

                max_triangle_edge_length *= max_triangle_edge_length; // actually squared distance
            }
        }

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        let start = Instant::now();

        let mut inputs = vec![];
        let mut outputs = vec![];
        if input_file.is_empty() {
            if working_directory.is_empty() {
                return Err(Error::new(ErrorKind::InvalidInput,
                    "This tool must be run by specifying either an individual input file or a working directory."));
            }
            if std::path::Path::new(&working_directory).is_dir() {
                for entry in fs::read_dir(working_directory.clone())? {
                    let s = entry?
                        .path()
                        .into_os_string()
                        .to_str()
                        .expect("Error reading path string")
                        .to_string();
                    if s.to_lowercase().ends_with(".las") {
                        inputs.push(s);
                        outputs.push(
                            inputs[inputs.len() - 1]
                                .replace(".las", ".tif")
                                .replace(".LAS", ".tif"),
                        )
                    } else if s.to_lowercase().ends_with(".zlidar") {
                        inputs.push(s);
                        outputs.push(
                            inputs[inputs.len() - 1]
                                .replace(".zlidar", ".tif")
                                .replace(".ZLIDAR", ".tif"),
                        )
                    } else if s.to_lowercase().ends_with(".zip") {
                        inputs.push(s);
                        outputs.push(
                            inputs[inputs.len() - 1]
                                .replace(".zip", ".tif")
                                .replace(".ZIP", ".tif"),
                        )
                    }
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("The input directory ({}) is incorrect.", working_directory),
                ));
            }
        } else {
            if !input_file.contains(path::MAIN_SEPARATOR) && !input_file.contains("/") {
                input_file = format!("{}{}", working_directory, input_file);
            }
            inputs.push(input_file.clone());
            if output_file.is_empty() {
                output_file = input_file
                    .clone()
                    .replace(".las", ".tif")
                    .replace(".LAS", ".tif")
                    .replace(".zlidar", ".tif");
            }
            if !output_file.contains(path::MAIN_SEPARATOR) && !output_file.contains("/") {
                output_file = format!("{}{}", working_directory, output_file);
            }
            outputs.push(output_file);
        }

        /*
        If multiple files are being interpolated, we will need to know their bounding boxes,
        in order to retrieve points from adjacent tiles. This is so that there are no edge
        effects.
        */
        let mut bounding_boxes = vec![];
        for in_file in &inputs {
            let header = LasHeader::read_las_header(&in_file.replace("\"", "")).expect(&format!(
                "Error while reading LiDAR file header ({}).",
                in_file
            ));
            bounding_boxes.push(BoundingBox {
                min_x: header.min_x,
                max_x: header.max_x,
                min_y: header.min_y,
                max_y: header.max_y,
            });
        }

        if verbose {
            println!("Performing interpolation...");
        }

        let num_tiles = inputs.len();
        let num_procs = num_cpus::get();
        let inputs = Arc::new(inputs);
        let outputs = Arc::new(outputs);
        let bounding_boxes = Arc::new(bounding_boxes);
        let (tx2, rx2) = mpsc::channel();
        for tid in 0..num_procs {
            let inputs = inputs.clone();
            let outputs = outputs.clone();
            let bounding_boxes = bounding_boxes.clone();
            let tool_name = self.get_tool_name();
            let tx2 = tx2.clone();
            thread::spawn(move || {
                for tile in (0..num_tiles).filter(|t| t % num_procs == tid) {
                    let start_run = Instant::now();

                    let input_file = inputs[tile].replace("\"", "").clone();
                    let output_file = outputs[tile].replace("\"", "").clone();

                    // Expand the bounding box to include the areas of overlap
                    let bb = BoundingBox {
                        min_x: bounding_boxes[tile].min_x - 2f64,
                        max_x: bounding_boxes[tile].max_x + 2f64,
                        min_y: bounding_boxes[tile].min_y - 2f64,
                        max_y: bounding_boxes[tile].max_y + 2f64,
                    };

                    let mut frs: FixedRadiusSearch2D<usize> =
                        FixedRadiusSearch2D::new(search_radius, DistanceMetric::Euclidean);

                    let mut points = vec![];
                    let mut z_values = vec![];

                    if verbose && inputs.len() == 1 {
                        println!("Reading input LiDAR file...");
                    }

                    let mut progress: i32;
                    let mut old_progress: i32 = -1;
                    let mut epsg_code = 0u16;
                    for m in 0..inputs.len() {
                        if bounding_boxes[m].overlaps(bb) {

                            match LasFile::new(&inputs[m].replace("\"", "").clone(), "r") {
                                Ok(input) => {
                                    let n_points = input.header.number_of_points as usize;
                                    let num_points: f64 =
                                        (input.header.number_of_points - 1) as f64; // used for progress calculation only
                                    epsg_code = input.get_epsg_code();

                                    for i in 0..n_points {
                                        let p: PointData = input[i];
                                        if !p.withheld() {
                                            if p.classification() != 7 && p.classification() != 18 { // it's not low or high noise
                                                if bb.is_point_in_box(p.x, p.y)
                                                    && p.z >= min_z
                                                    && p.z <= max_z
                                                {
                                                    frs.insert(p.x, p.y, z_values.len());
                                                    points.push(Point2D {
                                                        x: p.x,
                                                        y: p.y,
                                                    });
                                                    z_values.push(p.z);
                                                }
                                            }
                                        }
                                        if verbose && inputs.len() == 1 {
                                            progress =
                                                (100.0_f64 * (i+1) as f64 / num_points) as i32;
                                            if progress != old_progress {
                                                println!("Reading points: {}%", progress);
                                                old_progress = progress;
                                            }
                                        }
                                    }
                                },  
                                Err(_err) => {} // do nothing
                            };
                        }
                    }

                    if points.len() < 3 {
                        println!(
                            "Warning: No eligible points found in {}",
                            inputs[tile].clone()
                        );
                        tx2.send(tile).unwrap();
                    } else {
                        let num_points = points.len();
                        let mut remove_pt = vec![false; num_points];
                        let max_slope = 60f64.to_radians();
                        let height_threshold = max_slope.tan() * search_radius;
                        let mut z: f64;
                        let mut zn: f64;
                        let mut height_diff: f64;
                        let mut index: usize;

                        for i in 0..num_points {
                            z = z_values[i];
                            let ret = frs.search(points[i].x, points[i].y);
                            if ret.len() > 0 {
                                for j in 0..ret.len() {
                                    index = ret[j].0;
                                    zn = z_values[index];
                                    height_diff = z - zn;
                                    if height_diff > height_threshold {
                                        remove_pt[index] = true;
                                    } else if height_diff.abs() > height_threshold {
                                        remove_pt[i] = true;
                                    }
                                }
                            }
                            if verbose && inputs.len() == 1 {
                                progress =
                                    (100.0_f64 * (i+1) as f64 / num_points as f64) as i32;
                                if progress != old_progress {
                                    println!("Filtering points: {}%", progress);
                                    old_progress = progress;
                                }
                            }
                        }

                        // for i in (0..num_points).rev() {
                        //     if remove_pt[i] {
                        //         points.remove(i);
                        //         z_values.remove(i);
                        //     }
                        // }

                        let mut points2 = Vec::with_capacity(num_points);
                        let mut z_values2 = Vec::with_capacity(num_points);
                        for i in 0..num_points {
                            if !remove_pt[i] {
                                points2.push(points[i]);
                                z_values2.push(z_values[i]);
                            }
                        }
                        drop(frs);
                        drop(points);
                        drop(z_values);
                        drop(remove_pt);

                        let west: f64 = bounding_boxes[tile].min_x;
                        let north: f64 = bounding_boxes[tile].max_y;
                        let rows: isize =
                            (((north - bounding_boxes[tile].min_y) / grid_res).ceil()) as isize;
                        let columns: isize =
                            (((bounding_boxes[tile].max_x - west) / grid_res).ceil()) as isize;
                        let south: f64 = north - rows as f64 * grid_res;
                        let east = west + columns as f64 * grid_res;
                        let nodata = -32768.0f64;

                        let mut configs = RasterConfigs {
                            ..Default::default()
                        };
                        configs.rows = rows as usize;
                        configs.columns = columns as usize;
                        configs.north = north;
                        configs.south = south;
                        configs.east = east;
                        configs.west = west;
                        configs.resolution_x = grid_res;
                        configs.resolution_y = grid_res;
                        configs.nodata = nodata;
                        configs.data_type = DataType::F32;
                        configs.photometric_interp = PhotometricInterpretation::Continuous;
                        configs.epsg_code = epsg_code;
                        configs.projection = esri_wkt_from_epsg(epsg_code);

                        let mut output = Raster::initialize_using_config(&output_file, &configs);

                        // do the triangulation
                        if num_tiles == 1 && verbose {
                            println!("Performing triangulation...");
                        }
                        let result = triangulate(&points2).expect("No triangulation exists.");
                        let num_triangles = result.triangles.len() / 3;

                        let (mut p1, mut p2, mut p3): (usize, usize, usize);
                        let (mut top, mut bottom, mut left, mut right): (f64, f64, f64, f64);

                        let (mut top_row, mut bottom_row, mut left_col, mut right_col): (
                            isize,
                            isize,
                            isize,
                            isize,
                        );
                        let mut tri_points: Vec<Point2D> = vec![Point2D::new(0f64, 0f64); 4];
                        let mut k: f64;
                        let mut norm: Vector3<f64>;
                        let (mut a, mut b, mut c): (Vector3<f64>, Vector3<f64>, Vector3<f64>);
                        let (mut x, mut y): (f64, f64);
                        let mut i: usize;
                        for triangle in 0..num_triangles {
                            i = triangle * 3;
                            p1 = result.triangles[i];
                            p2 = result.triangles[i + 1];
                            p3 = result.triangles[i + 2];

                            if max_distance_squared(
                                points2[p1],
                                points2[p2],
                                points2[p3],
                                z_values2[p1],
                                z_values2[p2],
                                z_values2[p3],
                            ) < max_triangle_edge_length
                            {
                                tri_points[0] = points2[p1].clone();
                                tri_points[1] = points2[p2].clone();
                                tri_points[2] = points2[p3].clone();
                                tri_points[3] = points2[p1].clone();

                                // get the equation of the plane
                                a = Vector3::new(
                                    tri_points[0].x,
                                    tri_points[0].y,
                                    z_values2[p1],
                                );
                                b = Vector3::new(
                                    tri_points[1].x,
                                    tri_points[1].y,
                                    z_values2[p2],
                                );
                                c = Vector3::new(
                                    tri_points[2].x,
                                    tri_points[2].y,
                                    z_values2[p3],
                                );
                                norm = (b - a).cross(&(c - a));
                                k = -(tri_points[0].x * norm.x
                                    + tri_points[0].y * norm.y
                                    + norm.z * z_values2[p1]);

                                // find grid intersections with this triangle
                                bottom = points2[p1].y.min(points2[p2].y.min(points2[p3].y));
                                top = points2[p1].y.max(points2[p2].y.max(points2[p3].y));
                                left = points2[p1].x.min(points2[p2].x.min(points2[p3].x));
                                right = points2[p1].x.max(points2[p2].x.max(points2[p3].x));

                                bottom_row = ((north - bottom) / grid_res).ceil() as isize; // output.get_row_from_y(bottom);
                                top_row = ((north - top) / grid_res).floor() as isize; // output.get_row_from_y(top);
                                left_col = ((left - west) / grid_res).floor() as isize; // output.get_column_from_x(left);
                                right_col = ((right - west) / grid_res).ceil() as isize; // output.get_column_from_x(right);

                                for row in top_row..=bottom_row {
                                    for col in left_col..=right_col {
                                        x = west + col as f64 * grid_res;
                                        y = north - row as f64 * grid_res;
                                        if point_in_poly(&Point2D::new(x, y), &tri_points) {
                                            // calculate the z values
                                            zn = -(norm.x * x + norm.y * y + k) / norm.z;
                                            output.set_value(row, col, zn);
                                        }
                                    }
                                }

                                if verbose && num_tiles == 1 {
                                    progress = (100.0_f64 * triangle as f64
                                        / (num_triangles - 1) as f64)
                                        as i32;
                                    if progress != old_progress {
                                        println!("Progress: {}%", progress);
                                        old_progress = progress;
                                    }
                                }
                            }
                        }
                        
                        let elapsed_time_run = get_formatted_elapsed_time(start_run);
                        output.add_metadata_entry(format!(
                            "Created by whitebox_tools\' {} tool",
                            tool_name
                        ));
                        output.add_metadata_entry(format!("Input file: {}", input_file));
                        output.add_metadata_entry(format!("Grid resolution: {}", grid_res));
                        output.add_metadata_entry(format!(
                            "Elapsed Time (including I/O): {}",
                            elapsed_time_run
                        ));

                        if verbose && inputs.len() == 1 {
                            println!("Saving data...")
                        };

                        let _ = output.write().expect("Error writing file.");

                        tx2.send(tile).unwrap();
                    }
                }
            });
        }

        let mut progress: i32;
        let mut old_progress: i32 = -1;
        for tile in 0..inputs.len() {
            let tile_completed = rx2.recv().unwrap();
            if verbose {
                if tile <= 98 {
                    println!(
                        "Finished {} ({} of {})",
                        inputs[tile_completed]
                            .replace("\"", "")
                            .replace(working_directory, "")
                            .replace(".las", ""),
                        tile + 1,
                        inputs.len()
                    );
                } else if tile == 99 {
                    println!(
                        "Finished {} ({} of {})",
                        inputs[tile_completed]
                            .replace("\"", "")
                            .replace(working_directory, "")
                            .replace(".las", ""),
                        tile + 1,
                        inputs.len()
                    );
                    println!("...");
                }
                progress = (100.0_f64 * tile as f64 / (inputs.len() - 1) as f64) as i32;
                if progress != old_progress {
                    println!("Progress: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let elapsed_time = get_formatted_elapsed_time(start);

        if verbose {
            println!(
                "{}",
                &format!("Elapsed Time (including I/O): {}", elapsed_time)
            );
        }

        Ok(())
    }
}

/// Calculate squared Euclidean distance between the point and another.
pub fn max_distance_squared(
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    z1: f64,
    z2: f64,
    z3: f64,
) -> f64 {
    let mut dx = p1.x - p2.x;
    let mut dy = p1.y - p2.y;
    let mut dz = z1 - z2;
    let mut max_dist = dx * dx + dy * dy + dz * dz;

    dx = p1.x - p3.x;
    dy = p1.y - p3.y;
    dz = z1 - z3;
    let mut dist = dx * dx + dy * dy + dz * dz;

    if dist > max_dist {
        max_dist = dist
    }

    dx = p2.x - p3.x;
    dy = p2.y - p3.y;
    dz = z2 - z3;
    dist = dx * dx + dy * dy + dz * dz;

    if dist > max_dist {
        max_dist = dist
    }

    max_dist
}