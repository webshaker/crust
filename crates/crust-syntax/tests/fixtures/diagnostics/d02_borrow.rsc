pub class Pile {
    items: Vec<u32>,

    pub fn bug(&mut self) {
        let premier = &self.items[0];
        self.items.push(1); //~ ERROR E0502 self.items.push(1)
        println!("{premier}");
    }
}
