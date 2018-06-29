extern crate rusty_machine;
extern crate rulinalg;
extern crate csv;
extern crate regex;
extern crate image;

pub mod data_loader {

    use log_handler::data_loader::rusty_machine::linalg::Matrix;
    use log_handler::data_loader::rusty_machine::linalg::Vector;
    use log_handler::data_loader::csv;
    use log_handler::data_loader::regex::Regex;
    use log_handler::data_loader::image;
    use std::path::Path;
    use std::ops::Range;
    use std::collections::HashMap;

    pub trait DataLoader {
        type Feature;
        type Predictor;
        fn load_all_samples(&self, filepaths: Vec<(&str, Option<usize>)>) -> (Vec<Vec<Self::Feature>>, Vec<Self::Predictor>, HashMap<Option<usize>, Range<usize>>);
        fn vecs_as_matrix(&self, features: Vec<Vec<Self::Feature>>) -> Matrix<Self::Feature> {
            let samplecount = features.len();
            let featurescount = if samplecount > 0 {features[0].len()} else {0};
            let flattened_features = features.into_iter().flat_map(|x| x).collect();
            Matrix::new::<Vec<Self::Feature>>(samplecount, featurescount, flattened_features)
        }

        fn vec_as_vector(&self, predictors: Vec<Self::Predictor>) -> Vector<Self::Predictor> {
            Vector::new(predictors)
        }
    }

    pub fn new(dataformat_json_filepath: &Path) -> CSVLoader {
        // Default implementation for testing purposes.
        // TODO: Setup factory: Use the provided JSON file to return
        // a concrete implementation of the DataLoader trait (instead of the default CSVLoader here)
        CSVLoader{ 
            delimiter: b',',
            predictor_column_index: 4,
            has_headers: false
        }
    }

    pub struct CSVLoader {
        pub delimiter: u8,
        pub predictor_column_index: usize,
        pub has_headers: bool
    }

    impl DataLoader for CSVLoader{
        type Feature = f64;
        type Predictor = u32;
        fn load_all_samples(&self, filepaths: Vec<(&str, Option<usize>)>) -> (Vec<Vec<Self::Feature>>, Vec<Self::Predictor>, HashMap<Option<usize>, Range<usize>>) {
            
            let mut all_features = Vec::new();
            let mut all_predictors = Vec::new();
            let mut shard_attributions = HashMap::new();

            // Loop through all the paths and additionally keep track of which samples belong to which shards
            let mut sample_index = 0; // Sample counter
            let mut last_shard_opt = None;
            let mut is_first_iter = true;
            let mut last_shard_start_index = 0;
            
            for (path, shard_opt) in filepaths.iter() {
                if is_first_iter {
                    last_shard_opt = *shard_opt;
                    is_first_iter = false;
                } else if last_shard_opt != *shard_opt {
                    shard_attributions.insert(last_shard_opt, last_shard_start_index..sample_index);
                    last_shard_start_index = sample_index;
                    last_shard_opt = *shard_opt;
                }

                let mut rdr = csv::ReaderBuilder::new()
                                .has_headers(self.has_headers)
                                .delimiter(self.delimiter)
                                .flexible(false)
                                .from_path(path)
                                .unwrap();
                
                
                for rec in rdr.records() {
                    let rr = rec.unwrap();
                    let mut rec_vec = Vec::new();

                    for i in 0..rr.len() {
                        if i != self.predictor_column_index {
                            rec_vec.push(rr.get(i).unwrap().parse::<Self::Feature>().unwrap());
                        } else {
                            all_predictors.push(rr.get(i).unwrap().parse::<Self::Predictor>().unwrap());
                        }
                    }
                    all_features.push(rec_vec.to_owned());
                    sample_index += 1;
                }

                
            }

            if !is_first_iter {
                shard_attributions.insert(last_shard_opt, last_shard_start_index..sample_index);
            }

            (all_features, all_predictors, shard_attributions)
        }
    }

    pub struct ImageLoader {
        pub predictor_extract_regex_lookaround: Regex,
        pub predictor_extract_regex_exact: Regex
    }

    impl DataLoader for ImageLoader{
        type Feature = f64;
        type Predictor = u32;
        fn load_all_samples(&self, filepaths: Vec<(&str, Option<usize>)>) -> (Vec<Vec<Self::Feature>>, Vec<Self::Predictor>, HashMap<Option<usize>, Range<usize>>) {
            
            let mut all_features = Vec::new();
            let mut all_predictors = Vec::new();
            let mut shard_attributions = HashMap::new();
            
            // Loop through all the paths and additionally keep track of which samples belong to which shards
            let mut sample_index = 0; // Sample counter
            let mut last_shard_opt = None;
            let mut is_first_iter = true;
            let mut last_shard_start_index = 0;

            for (file_path, shard_opt) in filepaths.iter() {
                if is_first_iter {
                    last_shard_opt = *shard_opt;
                    is_first_iter = false;
                } else if last_shard_opt != *shard_opt {
                    shard_attributions.insert(last_shard_opt, last_shard_start_index..sample_index);
                    last_shard_start_index = sample_index;
                    last_shard_opt = *shard_opt;
                }

                let path = Path::new(file_path);
                let img = image::open(path).unwrap();
                
                all_features.push(img.raw_pixels().iter().map(|pix| *pix as f64).collect());
                
                let filename = (*path).file_name().unwrap().to_str().unwrap();
                let lookaround_match = self.predictor_extract_regex_lookaround.find(filename).unwrap().as_str();
                let extracted_predictor = self.predictor_extract_regex_exact.find(lookaround_match).unwrap().as_str();
                let predictor = extracted_predictor.parse::<Self::Predictor>().unwrap();

                all_predictors.push(predictor);
                sample_index += 1;
            }

            if !is_first_iter {
                shard_attributions.insert(last_shard_opt, last_shard_start_index..sample_index);
            }

            (all_features, all_predictors, shard_attributions)
        }
    }
}