use std::borrow::Borrow;
use crate::point::*;
use crate::simplex::*;
use crate::search_space::*;
use priority_queue::PriorityQueue;
use ordered_float::OrderedFloat;
use num_traits::Float;
use std::rc::Rc;

/// Stores the parameters and current state of a search.
///
/// - `ValueFloat` is the float type used to represent the evaluations (such as f64)
/// - `CoordFloat` is the float type used to represent the coordinates (such as f32)
pub struct Optimizer<CoordFloat: Float, ValueFloat: Float>
{
    exploration_depth: ValueFloat,
    minimize: bool,
//    f: Option<&'f_lifetime dyn Fn(&[CoordFloat]) -> ValueFloat>,
    search_space: SearchSpace<CoordFloat>,
    best_point: Rc<Point<CoordFloat, ValueFloat>>,
    min_value: ValueFloat,
    in_progress_simplex: Option<(usize, Simplex<CoordFloat, ValueFloat>)>,
    current_simplex: Option<Simplex<CoordFloat, ValueFloat>>,
    current_difference: Option<ValueFloat>,
    queue: PriorityQueue<Simplex<CoordFloat, ValueFloat>, OrderedFloat<ValueFloat>>
}

impl<CoordFloat: Float, ValueFloat: Float> Optimizer<CoordFloat, ValueFloat>
{
    /// Creates a new optimizer to explore the given search space with the iterator interface.
    ///
    /// Takes a function, a vector of intervals describing the input and a boolean describing wether it is a minimization problem (as oppozed to a miximization problem).
    /// Each cal to the `.next()` function (cf iterator trait) will run an iteration of search and output the best result so far.
    ///
    /// **Warning:** In d dimenssions, this function will perform d+1 evaluation (call to f) for the initialisation of the search (those should be taken into account when counting iterations).
    ///
    /// ```rust
    /// # use simplers_optimization::Optimizer;
    /// # fn main() {
    /// let f = |v:&[f64]| v[0] * v[1];
    /// let input_interval = vec![(-10., 10.), (-20., 20.)];
    /// let should_minimize = true;
    ///
    /// // runs the search for 30 iterations
    /// // then waits until we find a point good enough
    /// // finally stores the best value so far
    /// let (min_value, coordinates) = Optimizer::new(&f, &input_interval, should_minimize)
    ///                                          .skip(30)
    ///                                          .skip_while(|(value,coordinates)| *value > 1. )
    ///                                          .next().unwrap();
    ///
    /// println!("min value: {} found in [{}, {}]", min_value, coordinates[0], coordinates[1]);
    /// # }
    /// ```
    pub fn new(input_interval: &[(CoordFloat, CoordFloat)],
               should_minimize: bool)
               -> Self
    {
        // builds initial conditions
        let search_space = SearchSpace::new(input_interval);
        let initial_simplex = Simplex::initial_simplex(&search_space);

        // initialize priority queue
        // no need to evaluate the initial simplex as it will be poped immediatly
        let mut queue: PriorityQueue<Simplex<CoordFloat, ValueFloat>, OrderedFloat<ValueFloat>> =
            PriorityQueue::new();

        let exploration_depth = ValueFloat::from(6.).unwrap();
        Optimizer { minimize: should_minimize,
            exploration_depth,
            search_space,
            best_point: initial_simplex.corners[0].clone(),
            min_value: ValueFloat::zero(),
            in_progress_simplex: Some((0, initial_simplex)),
            current_simplex: None,
            queue,
            current_difference: None }
    }

    // pub fn with_fn(&mut self, f: &'f_lifetime impl Fn(&[CoordFloat]) -> ValueFloat) {
    //     self.f = Some(f);
    // }

    fn finalize_initial_simplex(&mut self) {
        if let Some((dim, simplex)) = self.in_progress_simplex.as_ref() {
            // various values track through the iterations
            let best_point = simplex.corners
                .iter()
                .max_by_key(|c| OrderedFloat(c.value))
                .expect("You need at least one dimension!")
                .clone();
            let min_value = simplex.corners
                .iter()
                .map(|c| c.value)
                .min_by_key(|&v| OrderedFloat(v))
                .expect("You need at least one dimension!");

            self.queue.push(simplex.clone(), OrderedFloat(ValueFloat::zero()));
            self.min_value = min_value;
            self.best_point = best_point;
        }

        self.in_progress_simplex = None;
    }

    fn step_in_progress_simplex(&mut self, value: ValueFloat) -> Option<Coordinates<CoordFloat>> {
        if let Some((dim, simplex)) = self.in_progress_simplex.as_mut() {
            let coordinates = simplex.corners[*dim].coordinates.clone();
            simplex.corners[*dim] = Rc::new(Point { coordinates, value });

            *dim += 1;
            if *dim < simplex.corners.len() {
                return Some(simplex.corners[*dim].coordinates.clone())
            }
        }

        self.finalize_initial_simplex();

        None
    }
        /// Sets the exploration depth for the algorithm, useful when using the iterator interface.
    ///
    /// `exploration_depth` represents the number of splits we can exploit before requiring higher-level exploration.
    /// As long as one stays in a reasonable range (5-10), the algorithm should not be very sensible to the parameter :
    ///
    /// - 0 represents full exploration (similar to grid search)
    /// - high numbers focus on exploitation (no need to go very high)
    /// - 5 appears to be a good default value
    ///
    /// **WARNING**: this function should not be used before after an iteration
    /// (as it will not update the score of already computed points for the next iterations
    /// which will degrade the quality of the algorithm)
    ///
    /// ```rust
    /// # use simplers_optimization::Optimizer;
    /// # fn main() {
    /// let f = |v:&[f64]| v[0] * v[1];
    /// let input_interval = vec![(-10., 10.), (-20., 20.)];
    /// let should_minimize = true;
    ///
    /// // sets exploration_depth to be very greedy
    /// let (min_value_greedy, _) = Optimizer::new(&f, &input_interval, should_minimize)
    ///                                          .set_exploration_depth(20)
    ///                                          .skip(100)
    ///                                          .next().unwrap();
    ///
    /// // sets exploration_depth to focus on exploration
    /// let (min_value_explore, _) = Optimizer::new(&f, &input_interval, should_minimize)
    ///                                          .set_exploration_depth(0)
    ///                                          .skip(100)
    ///                                          .next().unwrap();
    ///
    /// println!("greedy result : {} vs exploration result : {}", min_value_greedy, min_value_explore);
    /// # }
    /// ```
    pub fn set_exploration_depth(mut self, exploration_depth: usize) -> Self
    {
        self.exploration_depth = ValueFloat::from(exploration_depth + 1).unwrap();
        self
    }

    /// Self contained optimization algorithm.
    ///
    /// Takes a function to maximize, a vector of intervals describing the input and a number of iterations.
    ///
    /// ```rust
    /// # use simplers_optimization::Optimizer;
    /// # fn main() {
    /// let f = |v:&[f64]| v[0] + v[1];
    /// let input_interval = vec![(-10., 10.), (-20., 20.)];
    /// let nb_iterations = 100;
    ///
    /// let (max_value, coordinates) = Optimizer::maximize(&f, &input_interval, nb_iterations);
    /// println!("max value: {} found in [{}, {}]", max_value, coordinates[0], coordinates[1]);
    /// # }
    /// ```
    // pub fn maximize(f: &'f_lifetime impl Fn(&[CoordFloat]) -> ValueFloat,
    //                 input_interval: &[(CoordFloat, CoordFloat)],
    //                 nb_iterations: usize)
    //                 -> (ValueFloat, Coordinates<CoordFloat>)
    // {
    //     let initial_iteration_number = input_interval.len() + 1;
    //     let should_minimize = false;
    //     Optimizer::new(input_interval, should_minimize).nth(nb_iterations - initial_iteration_number)
    //         .unwrap().with_fn(f)
    // }

    /// Self contained optimization algorithm.
    ///
    /// Takes a function to minimize, a vector of intervals describing the input and a number of iterations.
    ///
    /// ```rust
    /// # use simplers_optimization::Optimizer;
    /// # fn main() {
    /// let f = |v:&[f64]| v[0] * v[1];
    /// let input_interval = vec![(-10., 10.), (-20., 20.)];
    /// let nb_iterations = 100;
    ///
    /// let (min_value, coordinates) = Optimizer::minimize(&f, &input_interval, nb_iterations);
    /// println!("min value: {} found in [{}, {}]", min_value, coordinates[0], coordinates[1]);
    /// # }
    /// ```
    // pub fn minimize(f: &'f_lifetime impl Fn(&[CoordFloat]) -> ValueFloat,
    //                 input_interval: &[(CoordFloat, CoordFloat)],
    //                 nb_iterations: usize)
    //                 -> (ValueFloat, Coordinates<CoordFloat>)
    // {
    //     let initial_iteration_number = input_interval.len() + 1;
    //     let should_minimize = true;
    //     Optimizer::new(input_interval, should_minimize).nth(nb_iterations - initial_iteration_number)
    //         .unwrap().with_fn(f)
    // }

    /// The next point which will be evaluated.
    /// Allows pre-empting function evaluation.
    pub fn next_explore_point(&mut self) -> Coordinates<CoordFloat> {
        if let Some((dim, simplex)) = self.in_progress_simplex.as_ref() {
            return simplex.corners[*dim].coordinates.clone()
        }

        // gets the exploration depth for later use
        let exploration_depth = self.exploration_depth;

        // gets an up to date simplex
        let mut simplex = self.queue.pop().expect("Impossible: The queue cannot be empty!").0;
        let current_difference = self.best_point.value - self.min_value;
        let mut n_iter = 0;
        let max_iter = self.queue.len();
        while (simplex.difference != current_difference) && (n_iter < max_iter)
        {
            simplex.difference = current_difference;
            let new_evaluation = simplex.evaluate(exploration_depth);
            let cleaned_evaluation = if new_evaluation >= ValueFloat::max_value() {
                self.best_point.value
            } else if new_evaluation <= ValueFloat::min_value() {
                self.min_value
            } else {
                new_evaluation
            };
            self.queue.push(simplex, OrderedFloat(new_evaluation));
            // pops a new simplex
            simplex = self.queue.pop().expect("Impossible: The queue cannot be empty!").0;
            n_iter += 1;
        }

        self.current_simplex = Some(simplex);
        self.current_difference = Some(current_difference);
        // evaluate the center of the simplex, then get it as a hypercube point
        self.search_space.to_hypercube(self.current_simplex.as_ref().unwrap().center.clone())
    }

    /// Allows avoiding lambda storage.
    pub fn next_with_value(&mut self, value: ValueFloat) -> (ValueFloat, Coordinates<CoordFloat>) {
        if self.in_progress_simplex.is_some(){
            let next_corner = self.step_in_progress_simplex(value);

            return (self.best_point.value, self.best_point.coordinates.clone());
        }

        let exploration_depth = self.exploration_depth;
        // evaluate the center of the simplex
        let simplex = if let Some(existing_simplex) = &self.current_simplex {
            // the next explore point has been calculated already
            existing_simplex
        } else {
            // need to calculate it first
            self.next_explore_point();
            self.current_simplex.as_ref().unwrap()
        }.clone();
        let current_difference = self.current_difference.unwrap();
        // current simplex is consumed
        self.current_simplex = None;
        self.current_difference = None;

        let coordinates= simplex.center.clone();

        let new_point = Rc::new(Point { coordinates, value });

        // splits the simplex around its center and push the subsimplex into the queue
        simplex.split(new_point.clone(), current_difference)
            .into_iter()
            .map(|s| (OrderedFloat(s.evaluate(exploration_depth)), s))
            .for_each(|(e, s)| {
                self.queue.push(s, e);
            });

        // updates the difference
        if value > self.best_point.value
        {
            self.best_point = new_point;
        }
        else if value < self.min_value
        {
            self.min_value = value;
        }

        // gets the best value so far
        let best_value =
            if self.minimize { -self.best_point.value } else { self.best_point.value };
        let best_coordinate = self.search_space.to_hypercube(self.best_point.coordinates.clone());
        (best_value, best_coordinate)
    }

}

// /// implements iterator for the Optimizer to give full control on the stopping condition to the user
// impl<'f_lifetime, CoordFloat: Float, ValueFloat: Float> Iterator
//     for Optimizer<'f_lifetime, CoordFloat, ValueFloat>
// {
//     type Item = (ValueFloat, Coordinates<CoordFloat>);
//
//     /// runs an iteration of the optimization algorithm and returns the best result so far
//     fn next(&mut self) -> Option<Self::Item>
//     {
//         let exploration_depth = self.exploration_depth;
//         // evaluate the center of the simplex
//         let simplex = if let Some(existing_simplex) = &self.current_simplex {
//             // the next explore point has been calculated already
//             existing_simplex
//         } else {
//             // need to calculate it first
//             self.next_explore_point();
//             self.current_simplex.as_ref().unwrap()
//         }.clone();
//         let current_difference = self.current_difference.unwrap();
//         // current simplex is consumed
//         self.current_simplex = None;
//         self.current_difference = None;
//
//         let coordinates= simplex.center.clone();
//
//         let value = self.search_space.evaluate(&coordinates);
//         let new_point = Rc::new(Point { coordinates, value });
//
//         // splits the simplex around its center and push the subsimplex into the queue
//         simplex.split(new_point.clone(), current_difference)
//                .into_iter()
//                .map(|s| (OrderedFloat(s.evaluate(exploration_depth)), s))
//                .for_each(|(e, s)| {
//                    self.queue.push(s, e);
//                });
//
//         // updates the difference
//         if value > self.best_point.value
//         {
//             self.best_point = new_point;
//         }
//         else if value < self.min_value
//         {
//             self.min_value = value;
//         }
//
//         // gets the best value so far
//         let best_value =
//             if self.search_space.minimize { -self.best_point.value } else { self.best_point.value };
//         let best_coordinate = self.search_space.to_hypercube(self.best_point.coordinates.clone());
//         Some((best_value, best_coordinate))
//     }
// }
