/* 
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 20/09/2018
Last Modified: 20/09/2018
License: MIT
*/

use std::env;
use std::f64;
use std::io::{Error, ErrorKind};
use std::path;
use time;
use tools::*;
use vector::*;

/// This can be used to identify the centroid point of a vector polyline or polygon feature or a group of
/// vector points. The output is a vector shapefile of points. For multi-part polyline or polygon features,
/// the user can optionally specify whether to identify the centroid of each part. The default is to treat
/// multi-part features a single entity.
///
/// See Also: CentroidVector
pub struct CentroidVector {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl CentroidVector {
    pub fn new() -> CentroidVector {
        // public constructor
        let name = "CentroidVector".to_string();
        let toolbox = "GIS Analysis".to_string();
        let description =
            "Identifes the centroid point of a vector polyline or polygon feature or a group of vector points."
                .to_string();

        let mut parameters = vec![];
        parameters.push(ToolParameter {
            name: "Input Vector File".to_owned(),
            flags: vec!["-i".to_owned(), "--input".to_owned()],
            description: "Input vector file.".to_owned(),
            parameter_type: ParameterType::ExistingFile(ParameterFileType::Vector(
                VectorGeometryType::Any,
            )),
            default_value: None,
            optional: false,
        });

        parameters.push(ToolParameter {
            name: "Output Vector File".to_owned(),
            flags: vec!["-o".to_owned(), "--output".to_owned()],
            description: "Output vector file.".to_owned(),
            parameter_type: ParameterType::NewFile(ParameterFileType::Vector(
                VectorGeometryType::Any,
            )),
            default_value: None,
            optional: false,
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
        let usage = format!(
            ">>.*{0} -r={1} -v --wd=\"*path*to*data*\" -i=in_file.shp -o=out_file.shp",
            short_exe, name
        ).replace("*", &sep);

        CentroidVector {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for CentroidVector {
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

        // read the arguments
        if args.len() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Tool run with no paramters.",
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
            }
        }

        let sep: String = path::MAIN_SEPARATOR.to_string();
        let mut progress: usize;
        let mut old_progress: usize = 1;

        let start = time::now();

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        if !input_file.contains(path::MAIN_SEPARATOR) && !input_file.contains("/") {
            input_file = format!("{}{}", working_directory, input_file);
        }

        if !output_file.contains(&sep) && !output_file.contains("/") {
            output_file = format!("{}{}", working_directory, output_file);
        }

        let input = Shapefile::read(&input_file)?;

        let (mut x_total, mut y_total): (f64, f64);

        if input.header.shape_type.base_shape_type() == ShapeType::Point {
            // create output file
            let mut output =
                Shapefile::initialize_using_file(&output_file, &input, ShapeType::Point, false)?;

            // add the attributes
            output
                .attributes
                .add_field(&AttributeField::new("FID", FieldDataType::Int, 2u8, 0u8));

            // read in the coordinates and find the median x and y coordinates
            x_total = 0f64;
            y_total = 0f64;

            for record_num in 0..input.num_records {
                let record = input.get_record(record_num);
                x_total += record.points[0].x;
                y_total += record.points[0].y;

                if verbose {
                    progress =
                        (100.0_f64 * (record_num + 1) as f64 / input.num_records as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            // output the medoid point
            x_total /= input.num_records as f64;
            y_total /= input.num_records as f64;

            output.add_point_record(x_total, y_total);
            output
                .attributes
                .add_record(vec![FieldData::Int(1i32)], false);

            if verbose {
                println!("Saving data...")
            };
            let _ = match output.write() {
                Ok(_) => if verbose {
                    println!("Output file written")
                },
                Err(e) => return Err(e),
            };
        } else {
            // create output file
            let mut output =
                Shapefile::initialize_using_file(&output_file, &input, ShapeType::Point, false)?;

            // add the attributes
            output
                .attributes
                .add_field(&AttributeField::new("FID", FieldDataType::Int, 2u8, 0u8));

            let mut num_points: usize;
            // output a medoid for each feature in the input file
            for record_num in 0..input.num_records {
                let record = input.get_record(record_num);
                num_points = record.points.len();
                x_total = 0f64;
                y_total = 0f64;
                for p in &record.points {
                    x_total += p.x;
                    y_total += p.y;
                }

                x_total /= num_points as f64;
                y_total /= num_points as f64;

                output.add_point_record(x_total, y_total);
                output
                    .attributes
                    .add_record(vec![FieldData::Int(record_num as i32 + 1i32)], false);

                if verbose {
                    progress =
                        (100.0_f64 * (record_num + 1) as f64 / input.num_records as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            if verbose {
                println!("Saving data...")
            };
            let _ = match output.write() {
                Ok(_) => if verbose {
                    println!("Output file written")
                },
                Err(e) => return Err(e),
            };
        }

        let end = time::now();
        let elapsed_time = end - start;

        if verbose {
            println!(
                "{}",
                &format!("Elapsed Time: {}", elapsed_time).replace("PT", "")
            );
        }

        Ok(())
    }
}