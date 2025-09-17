// Save as generate-jwt.js
const jwt = require('jsonwebtoken');

const JWT_SECRET = 'Qsfb9YUkdjwUULX.u96HdTCX4q7GuB';

// Match the Claims struct expected by your Rust code
const payload = {
  // Required field that's missing in your current token
  _id: "65648fa26b1eb500122c5323",  // Or whatever user ID format your system expects
  
  // Standard JWT fields
  sub: "65648fa26b1eb500122c5323",
  exp: Math.floor(Date.now() / 1000) + (60 * 60 * 24), // Expires in 24 hours
  iat: Math.floor(Date.now() / 1000), // Issued at time
  
  // Additional fields that might be required
//   role: "admin"
    email: "dev@integrationos.com",
    username: "integrationos-dev",
    userKey: "integrationos-dev522eb2",
    firstName: "IntegrationOS",
    lastName: "Developer",
    buildableId: "build-1c3cd7af757d4aebab523f5373190e1b",
    containerId: "",
    pointers: [
        "_1_3pejYG_SdSxV9xkt5_GA8WoMsSnfBHvY1qpGhlX-6DKd9kyZO3ee9hWfjGWpt5dY0AzxvM51q6_45_Q6bJTWCTuax7yq4X96nhvB0uTwhhLlsxyJm02JqasmdeDVeHt08GxGPoiBc7I9u00-1EKOejw62kNO0M1EaEFqwaGXw1Y8IfFH",
        "_1_hUOSWuG8lfzaWIvyA4NLf3YuuFIF_4oCzEF0nuKDiqyh0IA9yhIqcrkeBOsg8AhY509EdqufSPWEvuNpwib4puQLEbrJM55H2pSgHcFji-TLPT5HvqA24TNCpJcd70oAfgLsIAqmqmM8EJVyJQaa44stNUBWF6Ahg47P1KcFwFAJ0I_O"
      ],
    isBuildableCore: false,
    aud: "pica-users",
    iss: "pica"
};

const compositeSecret = JWT_SECRET + payload.buildableId;
const token = jwt.sign(payload, compositeSecret);
// const token = jwt.sign(payload, JWT_SECRET);
console.log(token);
