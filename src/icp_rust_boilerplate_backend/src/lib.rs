#[macro_use]
    extern crate serde;
    use candid::{Decode, Encode};
    use ic_cdk::api::time;
    use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
    use ic_stable_structures::{BoundedStorable, Cell, DefaultMemoryImpl, StableBTreeMap, Storable};
    use std::{borrow::Cow, cell::RefCell};
    use ic_cdk::caller;
    use candid::Principal;

    type Memory = VirtualMemory<DefaultMemoryImpl>;
    type IdCell = Cell<u64, Memory>;

    
    // Define the Event struct with CandidType, Clone, Serialize, Deserialize, and Default traits
    #[derive(candid::CandidType, Clone, Serialize, Deserialize, Default)]
    struct Event {
        id: u64,
        event_description: String,
        owner: String,
        event_title: String,
        event_location : String,
        event_card_imgurl : String,
        attendees : Vec<String>,
        created_at: u64,
        updated_at: Option<u64>,
    }

     // a trait that must be implemented for a struct that is stored in a stable struct
     impl Storable for Event {
        fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
            Cow::Owned(Encode!(self).unwrap())
        }
    
        fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
            Decode!(bytes.as_ref(), Self).unwrap()
        }
    }
    
    // another trait that must be implemented for a struct that is stored in a stable struct
    impl BoundedStorable for Event {
        const MAX_SIZE: u32 = 1024;
        const IS_FIXED_SIZE: bool = false;
    }



    thread_local! {
        static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
            MemoryManager::init(DefaultMemoryImpl::default())
        );
    
        static ID_COUNTER: RefCell<IdCell> = RefCell::new(
            IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
                .expect("Cannot create a counter")
        );

    
        static STORAGE: RefCell<StableBTreeMap<u64, Event, Memory>> =
            RefCell::new(StableBTreeMap::init(
                MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1)))
        ));
    }


    // Event payload for creating or updating an Event
    #[derive(candid::CandidType, Serialize, Deserialize, Default)]
    struct EventPayload {
        event_description: String,
        event_title: String,
        event_location : String,
        event_card_imgurl : String,
    }


    // Query function to retrieve details of a specific event by its unique identifier
    #[ic_cdk::query]
    fn get_event(id: u64) -> Result<Event, Error> {
        
        // Attempt to retrieve the event using the internal helper function
        match _get_event(&id) {
            // If the event is found, return it as a Result::Ok
            Some(message) => Ok(message),

            // If the event is not found, return a Result::Err with a NotFound error
            None => Err(Error::NotFound {
                msg: format!("Event with id={} not found", id),
            }),
        }
    }

    
    // Function to create a new event based on the provided payload
    #[ic_cdk::update]
    fn create_event(payload: EventPayload) -> Option<Event> {
        // Increment the unique identifier for the new event
        let id = ID_COUNTER
            .with(|counter| {
                let current_value = *counter.borrow().get();
                counter.borrow_mut().set(current_value + 1)
            })
            .expect("cannot increment id counter");

        // Create a new Event instance with the provided payload and additional details        
        let event = Event {
            id,
            event_description: payload.event_description,
            owner: caller().to_string(),
            event_title: payload.event_title,
            event_location : payload.event_location,
            event_card_imgurl : payload.event_card_imgurl,
            attendees : Vec::new(),
            created_at: time(),
            updated_at: None,
        };

        // Insert the newly created event into the storage
        do_insert(&event);

        // Return the newly created event as an Option
        Some(event)
    }


    // Update function to modify the details of an existing event
    #[ic_cdk::update]
    fn update_event(id: u64, payload: EventPayload) -> Result<Event, Error> {
    
    // Check if the caller is the owner of the event; if not, return an authorization error
    if !_check_if_owner(&_get_event(&id).unwrap().clone()){
        return Err(Error::NotAuthorized {
            msg: format!(
                "You're not the owner of the event with id={}",
                id
            ),
            caller: caller()
        })
    }

        // Attempt to retrieve the event from storage based on its unique identifier
        match STORAGE.with(|service| service.borrow().get(&id)) {
           
            Some(mut event) => {

                // Update event details with the provided payload
                event.event_description = payload.event_description;
                event.event_title = payload.event_title;
                event.event_location  = payload.event_location;
                event.event_card_imgurl  = payload.event_card_imgurl;
                event.updated_at = Some(time());
                
                // Insert the modified event back into storage
                do_insert(&event);
                Ok(event)
            }

            // If the event is not found, return a NotFound error
            None => Err(Error::NotFound {
                msg: format!(
                    "couldn't update an event with id={}. event not found",
                    id
                ),
            }),
        }
    }


    // Update function to add an attendee to a specific event
    #[ic_cdk::update]
    fn attend_event(id: u64) -> Result<Event, Error> {
    
    // Attempt to retrieve the event from storage based on its unique identifier
    match STORAGE.with(|service| service.borrow().get(&id)) {
        Some(mut event) => {
            // Get the caller's identity as an attendee
            let attendee = caller().to_string();
            
            // Retrieve the current list of attendees for the event
            let mut attendees: Vec<String> = event.attendees;

            // Check if that caller is already in the attendees list
            if attendees.contains(&attendee) {
                // Return an error message
                Err(Error::NotFound {
                    msg: format!("You are already an attendee"),
                })
            } else {
                attendees.push(attendee);
                event.attendees = attendees;

                do_insert(&event);
                // Return the modified event on success
                Ok(event)
            }
        }

        // If the event is not found, return a NotFound error
        None => Err(Error::NotFound {
            msg: format!("Couldn't update an event with id={}. Event not found", id),
        }),
    }
}



    // Update function to delete a specific event by its unique identifier
    #[ic_cdk::update]
    fn delete_event(id: u64) -> Result<Event, Error> {
    
    // Check if the caller is the owner of the event; if not, return an authorization error
    if !_check_if_owner(&_get_event(&id).unwrap().clone()){
        return Err(Error::NotAuthorized {
            msg: format!(
                "You're not the owner of the event with id={}",
                id
            ),
            caller: caller()
        })
    }

    // Attempt to remove the event from storage based on its unique identifier
    match STORAGE.with(|service| service.borrow_mut().remove(&id)) {
        
        // If the event is found and removed, return it as a Result::Ok
        Some(event) => Ok(event),

        // If the event is not found, return a Result::Err with a NotFound error
        None => Err(Error::NotFound {
            msg: format!(
                "couldn't delete an event with id={}. To-do not found.",
                id
            ),
            }),
        }
    }


    // Enum representing various error scenarios that can occur during event operations
    #[derive(candid::CandidType, Deserialize, Serialize)]
    enum Error {
        // Indicates that the requested event was not found
        NotFound { msg: String },

        // Indicates an authorization error when the caller is not the owner of the event
        NotAuthorized {msg: String , caller: Principal},
    }


     // Helper method to insert an event.
     fn do_insert(event: &Event) {
        STORAGE.with(|service| service.borrow_mut().insert(event.id, event.clone()));
    }

    // Helper method to retrieve an event by it's id 
    fn _get_event(id: &u64) -> Option<Event> {
        STORAGE.with(|s| s.borrow().get(id))
    }
    
    // Helper function to check whether the caller is the owner of the event
    fn _check_if_owner(event: &Event) -> bool {
    if event.owner.to_string() != caller().to_string(){
        false  
    }else{
        true
    }
}



    // need this to generate candid
    ic_cdk::export_candid!();


    